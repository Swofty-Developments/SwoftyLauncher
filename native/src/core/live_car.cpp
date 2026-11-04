#include "live_car.hpp"

#include "postfx_logic.hpp"
#include "runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cmath>
#include <cstdlib>

namespace live_car {
namespace {

namespace mem = swofty::mem;

constexpr size_t CAR_IS_AI = 0x124, CAR_ENGINE = 0x530, CAR_GEARBOX = 0x538, CAR_CHASSIS = 0x558, CAR_STEERING = 0x560,
                 CAR_SUSPENSION = 0x580, CAR_THROTTLE = 0x188, CAR_BRAKE = 0x190, CAR_FLAGS = 0x224;
constexpr size_t GEARBOX_INDEX = 0x680, ENGINE_RATE = 0x2538, ENGINE_LIMIT = 0x2528;
constexpr size_t CHASSIS_G_LATERAL = 0x320, CHASSIS_G_LONG = 0x324, CHASSIS_BODY = 0x340, BODY_STATE = 0x18,
                 BODY_VELOCITY = 0x220;
constexpr size_t STEER_ANGLE = 0xe8, STEER_LOCK = 0x28, SUSPENSION_WHEELS = 0x70, WHEEL_SURFACE = 0x200,
                 SURFACE_ID = 0x78;
constexpr int MAX_CARS = 26;
constexpr float RAD_S_TO_RPM = 60.0f / 6.2831855f;

uintptr_t table = 0, count = 0;
bool resolved = false;
int last_player = -1;

void resolve()
{
    resolved = true;
    if (!runtime::same_build()) {
        swofty::log("[shaders] live car signals are unavailable: this is not the build the car offsets are from");
        return;
    }
    if (swofty::api) {
        table = swofty::api->resolve("g_Sess_VehicleTable");
        count = swofty::api->resolve("g_Sess_VehicleCount");
    }
    if (!table || !count) {
        table = count = 0;
        swofty::log("[shaders] live car signals are unavailable: g_Sess_VehicleTable or g_Sess_VehicleCount does not "
                    "resolve");
    }
}

uintptr_t car_at(int i)
{
    uintptr_t car = 0;
    if (!mem::read(table + 8 * uintptr_t(i), car) || !car || !mem::readable(car, CAR_IS_AI + 1))
        return 0;
    return car;
}

uintptr_t ptr(uintptr_t base, size_t off)
{
    uintptr_t p = 0;
    return mem::read(base + off, p) ? p : 0;
}

bool real(float v)
{
    return std::isfinite(v);
}

bool xor_float(uintptr_t at, float &out)
{
    uint32_t a, b;
    if (!mem::read(at, a) || !mem::read(at + 8, b))
        return false;
    a ^= b;
    __builtin_memcpy(&out, &a, 4);
    return real(out);
}

int find_player(uintptr_t &car, bool &ai)
{
    uint32_t n = 0;
    if (!mem::read(count, n))
        return -1;
    if (n > MAX_CARS)
        n = MAX_CARS;
    static const int forced = [] {
        char v[16] = {};
        return GetEnvironmentVariableA("SWOFTY_LIVE_CAR_SLOT", v, sizeof v) && v[0] ? std::atoi(v) : -1;
    }();
    if (forced >= 0 && uint32_t(forced) < n) {
        uintptr_t c = car_at(forced);
        uint8_t is_ai = 1;
        if (c) {
            mem::read(c + CAR_IS_AI, is_ai);
            car = c;
            ai = is_ai != 0;
            return forced;
        }
    }
    if (last_player >= 0 && uint32_t(last_player) < n) {
        uintptr_t c = car_at(last_player);
        uint8_t is_ai = 0;
        if (c && mem::read(c + CAR_IS_AI, is_ai) && !is_ai) {
            car = c;
            ai = false;
            return last_player;
        }
    }
    for (uint32_t i = 0; i < n; i++) {
        uintptr_t c = car_at(int(i));
        uint8_t is_ai = 1;
        if (c && mem::read(c + CAR_IS_AI, is_ai) && !is_ai) {
            last_player = int(i);
            car = c;
            ai = false;
            return last_player;
        }
    }
    if (last_player >= 0 && uint32_t(last_player) < n) {
        uintptr_t c = car_at(last_player);
        if (c) {
            car = c;
            ai = true;
            return last_player;
        }
    }
    return -1;
}

void read_surface(uintptr_t car, LiveCar &out)
{
    uintptr_t susp = ptr(car, CAR_SUSPENSION);
    if (!susp)
        return;
    uint8_t game[4];
    for (int i = 0; i < 4; i++) {
        game[i] = 12;
        uintptr_t wheel = ptr(susp, SUSPENSION_WHEELS + 8 * size_t(i));
        if (!wheel)
            return;
        uintptr_t rec = ptr(wheel, WHEEL_SURFACE);
        uint8_t id = 0;
        uint32_t wide = 0;
        if (rec && mem::read(rec + SURFACE_ID, wide)) {
            id = uint8_t(wide > 12 ? 12 : wide);
            game[i] = id;
        }
    }
    postfx::wheels_contract_order(game, out.surface);
    out.has_surface = true;
}

}

bool read(LiveCar &out)
{
    out = LiveCar{};
    if (!resolved)
        resolve();
    if (!table || !count)
        return false;
    uintptr_t car = 0;
    bool ai = false;
    int player = find_player(car, ai);
    if (player < 0)
        return false;
    out.player = player;
    out.ai = ai;

    uintptr_t gearbox = ptr(car, CAR_GEARBOX), engine = ptr(car, CAR_ENGINE), chassis = ptr(car, CAR_CHASSIS);
    uint32_t gear_index = 0;
    bool has_gear_index = gearbox && mem::read(gearbox + GEARBOX_INDEX, gear_index);
    if (has_gear_index && gear_index < 16) {
        out.gear = int(gear_index) - 1;
        out.has_gear = true;
    }

    if (chassis) {
        uintptr_t body = ptr(ptr(chassis, CHASSIS_BODY), BODY_STATE);
        float v[3];
        if (body && mem::read(body + BODY_VELOCITY, v) && real(v[0]) && real(v[1]) && real(v[2])) {
            out.speed_kmh = std::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) * 3.6f;
            out.has_speed = true;
        }
        float gl, gf;
        if (mem::read(chassis + CHASSIS_G_LATERAL, gl) && mem::read(chassis + CHASSIS_G_LONG, gf) && real(gl) && real(gf)) {
            out.g_lateral = gl;
            out.g_long = gf;
            out.has_g = true;
        }
    }

    float thr, brk;
    uint8_t flags = 0;
    if (has_gear_index && mem::read(car + CAR_THROTTLE, thr) && mem::read(car + CAR_BRAKE, brk) &&
        mem::read(car + CAR_FLAGS, flags) && real(thr) && real(brk)) {
        if (gear_index == 0 && (flags & 0x88) == 8)
            std::swap(thr, brk);
        out.throttle = thr;
        out.brake = brk;
        out.has_pedals = true;
    }

    if (uintptr_t steering = ptr(car, CAR_STEERING)) {
        float angle, lock;
        if (xor_float(steering + STEER_ANGLE, angle) && xor_float(steering + STEER_LOCK, lock) && lock != 0) {
            out.steer = std::fmax(-1.0f, std::fmin(1.0f, -angle / lock));
            out.has_steer = true;
        }
    }

    if (engine) {
        float rate, limit;
        if (mem::read(engine + ENGINE_RATE, rate) && mem::read(engine + ENGINE_LIMIT, limit) && real(rate) && real(limit) &&
            limit > 0) {
            out.rpm = rate * RAD_S_TO_RPM;
            out.max_rpm = limit * RAD_S_TO_RPM;
            out.has_rpm = true;
        }
    }

    read_surface(car, out);
    return true;
}

}
