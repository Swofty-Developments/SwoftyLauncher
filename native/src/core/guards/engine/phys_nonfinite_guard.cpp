#include "core/guards/guard.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/engine/phys_nonfinite_logic.hpp"
#include "core/live/live_car_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <mutex>

// Physics non-finite log (re/bindings/core/Ne_PhysUpdateStage_FinishStepNotifyBodies.json).
//
// A physics update ends in Ne_PhysUpdateStage_FinishStepNotifyBodies: it pumps the world's step state machine (world
// = stage +0x8, state +0xcf0) until the last step the update started is done, then hands the constraints that have a
// step callback their impulse lengths. The game never checks a body for a value that is not a number. Crash reports put
// the first one in a car's body within 8-37 ms of a wheel coming off; from there it spreads to other cars, survives a
// session restart and later faults the tyre curve lookup and the steering clip sampler (the tyre curve and anim clip
// time guards).
//
// The log wraps the function. When a step was pending before the game's code ran, it reads the world's rigid body list
// afterwards (world +0x48, next body +0x330) in one pass that ORs a non-finite test over each body's position (+0x1c0),
// angular velocity (+0x210) and linear velocity (+0x220), and tests the result once at the end. Only a step that found
// one reads the list again, to log each such body once: its fields, its inverse mass and handle, the vehicle whose
// chassis it is and that car's wheels. It writes nothing to the game. A line every 60 s gives the time its reads took.
//
// SWOFTY_PHYS_NONFINITE_LOG=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (physics non-finite log)";
constexpr const char *FINISH_STEP = "Ne_PhysUpdateStage_FinishStepNotifyBodies";
constexpr const char *VEHICLE_TABLE = "g_Sess_VehicleTable";
constexpr const char *VEHICLE_COUNT = "g_Sess_VehicleCount";

constexpr size_t CAR_VEHICLE_ID = 0xe8, CAR_IS_AI = 0x124, CAR_CHASSIS = 0x558, CAR_SUSPENSION = 0x580;
constexpr size_t CHASSIS_BODY = 0x340, SUSPENSION_WHEELS = 0x70;
constexpr size_t WHEEL_SPIN = 0x188, WHEEL_SLIP_ANGLE = 0x3b0, WHEEL_SLIP_RATIO = 0x3b4;
constexpr uint32_t MAX_CARS = 64;

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;
namespace pn = runtime::phys_nonfinite;

using FinishStep = void(void *);
FinishStep *orig_finish_step = nullptr;

constexpr const char *GATE_BINDINGS[] = {FINISH_STEP};
const runtime::build_proof::Gate GATE{"the physics non-finite log", &pn::BODY_LIST, GATE_BINDINGS};

uintptr_t vehicle_table = 0, vehicle_count = 0;
bool car_table = false, wheel_fields = false;

uintptr_t ptr(uintptr_t base, size_t off)
{
    uintptr_t p = 0;
    return base && mem::read(base + off, p) ? p : 0;
}

pn::Owner owner_of(uintptr_t body)
{
    pn::Owner o;
    o.table = car_table && vehicle_table && vehicle_count;
    uint32_t n = 0;
    if (!o.table || !mem::read(vehicle_count, n))
        return o;
    for (uint32_t i = 0; i < n && i < MAX_CARS; i++) {
        uintptr_t car = ptr(vehicle_table, 8 * size_t(i));
        if (!car || ptr(ptr(ptr(car, CAR_CHASSIS), CHASSIS_BODY), pn::HANDLE_BODY) != body)
            continue;
        o.slot = int(i);
        uint8_t id = 0xff, ai = 0;
        mem::read(car + CAR_VEHICLE_ID, id);
        mem::read(car + CAR_IS_AI, ai);
        o.vehicle_id = id;
        o.ai = ai != 0;
        uintptr_t susp = wheel_fields ? ptr(car, CAR_SUSPENSION) : 0;
        if (!susp)
            return o;
        o.wheels = true;
        for (int w = 0; w < 4; w++) {
            uintptr_t wheel = ptr(susp, SUSPENSION_WHEELS + 8 * size_t(w));
            pn::Corner &c = o.corners[w];
            c.read = wheel && mem::read(wheel + WHEEL_SPIN, c.spin) && mem::read(wheel + WHEEL_SLIP_ANGLE, c.slip_angle) &&
                     mem::read(wheel + WHEEL_SLIP_RATIO, c.slip_ratio);
        }
        return o;
    }
    return o;
}

void log_line(const std::string &line) { swofty::log("%s", line.c_str()); }

constinit pn::Checker checker{&log_line, &owner_of};

std::mutex report_lock;
LARGE_INTEGER qpf{}, window_qpc{};
uint64_t window_tsc = 0;
bool window_open = false;

void report()
{
    std::unique_lock<std::mutex> hold(report_lock, std::try_to_lock);
    if (!hold.owns_lock())
        return;
    LARGE_INTEGER q;
    QueryPerformanceCounter(&q);
    uint64_t now = __builtin_ia32_rdtsc();
    if (!window_open) {
        QueryPerformanceFrequency(&qpf);
        window_qpc = q;
        window_tsc = now;
        window_open = true;
        checker.take();
        return;
    }
    double seconds = double(q.QuadPart - window_qpc.QuadPart) / double(qpf.QuadPart);
    if (seconds < pn::REPORT_SECONDS || now <= window_tsc)
        return;
    double ns_per_tick = seconds * 1e9 / double(now - window_tsc);
    pn::Window w = checker.take();
    window_qpc = q;
    window_tsc = now;
    swofty::log("%s", pn::perf_line(w, seconds, ns_per_tick).c_str());
}

void finish_step(void *stage)
{
    uintptr_t world = *reinterpret_cast<const uintptr_t *>(reinterpret_cast<uintptr_t>(stage) + pn::STAGE_WORLD);
    bool pending = world && *reinterpret_cast<const uint32_t *>(world + pn::WORLD_STATE) != 0;
    orig_finish_step(stage);
    if (!pending)
        return;
    uint64_t ticks = 0;
    checker.step(world, ticks);
    if (checker.steps() % pn::REPORT_CHECK_STEPS == 1)
        report();
}

void armed()
{
    car_table = runtime::build_proof::proven_on(live_car::CAR_TABLE, runtime::running_build());
    wheel_fields = runtime::build_proof::proven_on(live_car::WHEEL_FIELDS, runtime::running_build());
    if (car_table && swofty::api) {
        vehicle_table = swofty::api->resolve(VEHICLE_TABLE);
        vehicle_count = swofty::api->resolve(VEHICLE_COUNT);
    }
    if (car_table && (!vehicle_table || !vehicle_count))
        swofty::log("[phys] non-finite log: g_Sess_VehicleTable or g_Sess_VehicleCount does not resolve on this build, so "
                    "its lines do not name the car a body belongs to");
}

class PhysNonfiniteLog final : public guards::SiteGuard {
public:
    PhysNonfiniteLog()
        : SiteGuard(guards::CoreSetId::phys_nonfinite,
                    {
                        .set = MIXIN,
                        .name = "[phys] non-finite log",
                        .env = "SWOFTY_PHYS_NONFINITE_LOG",
                        .needs = {FINISH_STEP},
                        .mixins = {{FINISH_STEP, SWOFTY_MIXIN_WRAP, 100, (void *)&finish_step, (void **)&orig_finish_step}},
                        .armed = "each physics body that holds a value that is not finite after a step is logged once",
                        .on_armed = &armed,
                    })
    {
    }

private:
    bool admit() override { return runtime::holds(GATE); }
};

PhysNonfiniteLog guard;

}
