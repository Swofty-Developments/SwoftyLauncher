#pragma once
#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <functional>
#include <mutex>
#include <vector>

#include <swofty_api.h>

// The core's live car operations (SwoftyCoreCarOps, standard library 1.6): cars::live reads and writes a
// car's handling values while it is on track. A car is a slot of g_Sess_VehicleTable (its CarLive, whose
// vehicle_id is the slot); each value is a field of one of the systems the car caches by section name at
// CarLive+0x530..+0x5e0. Every offset below is a KB field (re/db/structs/{CarLive,CarSteering,CarGearBox,
// CarDifferential,CarTurbo,CarEngine,CarFuelTank,CarDamageFrontWing,CarDamageSuspension,CarObfFloat}.h, each
// exported by bindgen: L2 with an agree verify row, or L3), and "obf" fields are CarObfFloat pairs: the float's
// bits XOR the key 8 bytes on. The offsets are the KB build's, so car_live.cpp hands over no table on any
// other exe. Ops<Tag> keeps the state per tag so the host test runs the same code on fake cars.

namespace swofty_core::hooks::car_live {

constexpr uintptr_t CAR_VEHICLE_ID = 0xe8, CAR_IS_AI = 0x124, CAR_LOADER = 0x4a0, SYSTEM_CAR = 0x08;
constexpr uintptr_t FUEL_CAPACITY = 0x98;
constexpr uint32_t MAX_CARS = 32;

constexpr uintptr_t ENGINE = 0x530, STEERING = 0x560, GEARBOX = 0x538,
                    DIFFERENTIAL = 0x578, TURBO = 0x5a0, FUEL_TANK = 0x540,
                    WING_DAMAGE = 0x5c8, SUSPENSION_DAMAGE = 0x5d0;

enum class Kind { plain, obf };

// What set re-runs after the write so the game's derived copy follows: the engine's derived values
// (Car_Engine_ComputeDerived_Thunk) or the gearbox's ratio table (Car_Tuning_SetFinalDrive with 0x20|0x2000, which
// leaves the current gear alone; 0x200 would shift to neutral).
enum class Rederive { none, engine, gearbox };
constexpr uintptr_t GEARBOX_USE_SETUP_RATIOS = 0x540;
constexpr uint32_t GEARBOX_REBUILD = 0x2020;

struct Field {
    uint32_t id;
    const char *name;
    uintptr_t system, offset;
    Kind kind;
    float lo, hi;
    Rederive rederive = Rederive::none;
};

inline constexpr float BIG = 1e9f;

inline constexpr Field FIELDS[] = {
    {SWOFTY_CAR_STEERING_LOCK, "steering.max_steering_lock", STEERING, 0x28, Kind::obf, 0.01f, 1.5708f},
    {SWOFTY_CAR_CLUTCH_TORQUE, "gearbox.max_clutch_torque", GEARBOX, 0xe8, Kind::plain, 0, 100},
    {SWOFTY_CAR_DIFF_ON_THROTTLE, "differential.diff_on_throttle", DIFFERENTIAL, 0xf0, Kind::plain, 0, 1},
    {SWOFTY_CAR_DIFF_OFF_THROTTLE, "differential.diff_off_throttle", DIFFERENTIAL, 0xec, Kind::plain, 0, 1},
    {SWOFTY_CAR_TURBO_MAX_BOOST, "turbo.max_boost", TURBO, 0x2d8, Kind::plain, 0, 100},
    {SWOFTY_CAR_FUEL_MASS, "fuel_tank.fuel_mass", FUEL_TANK, 0x100, Kind::plain, 0, BIG},
    {SWOFTY_CAR_WING_SPEED_THRESHOLD, "f1_wing_damage.speed_threshold", WING_DAMAGE, 0x28, Kind::plain, 0, BIG},
    {SWOFTY_CAR_WING_DAMAGE_SCALE, "f1_wing_damage.damage_scale", WING_DAMAGE, 0x2c, Kind::plain, 0, BIG},
    {SWOFTY_CAR_WING_CHASSIS_DAMAGE_MULTIPLE, "f1_wing_damage.chassis_damage_multiple", WING_DAMAGE, 0x30, Kind::plain,
     0, BIG},
    {SWOFTY_CAR_WING_SIDE_DETACH_SCALE, "f1_wing_damage.side_detach_scale", WING_DAMAGE, 0x34, Kind::plain, 0, BIG},
    {SWOFTY_CAR_WING_FULL_DETACH_SCALE, "f1_wing_damage.full_detach_scale", WING_DAMAGE, 0x38, Kind::plain, 0, BIG},
    {SWOFTY_CAR_SUSP_SPEED_THRESHOLD, "f1_suspension_damage.speed_threshold", SUSPENSION_DAMAGE, 0x28, Kind::obf, 0,
     BIG},
    {SWOFTY_CAR_SUSP_FRONT_DAMAGE_SCALE, "f1_suspension_damage.front_damage_scale", SUSPENSION_DAMAGE, 0x38, Kind::obf,
     0, BIG},
    {SWOFTY_CAR_SUSP_REAR_DAMAGE_SCALE, "f1_suspension_damage.rear_damage_scale", SUSPENSION_DAMAGE, 0x48, Kind::obf, 0,
     BIG},
    {SWOFTY_CAR_SUSP_WHEEL_DETACH_SCALE, "f1_suspension_damage.wheel_detach_scale", SUSPENSION_DAMAGE, 0x58, Kind::obf,
     0, BIG},
    {SWOFTY_CAR_SUSP_PARTS_DETACH_SCALE, "f1_suspension_damage.parts_detach_scale", SUSPENSION_DAMAGE, 0x68, Kind::obf,
     0, BIG},
    {SWOFTY_CAR_SUSP_LINEAR_FRACTURE_LIMIT, "f1_suspension_damage.linear_fracture_limit", SUSPENSION_DAMAGE, 0x78,
     Kind::plain, 0, BIG},
    {SWOFTY_CAR_SUSP_ANGULAR_FRACTURE_LIMIT, "f1_suspension_damage.angular_fracture_limit", SUSPENSION_DAMAGE, 0x7c,
     Kind::plain, 0, BIG},
    {SWOFTY_CAR_ENGINE_LIMITER, "engine.vtf_limiter_rotation_rate", ENGINE, 0x8b0, Kind::obf, 100, 2500,
     Rederive::engine},
    {SWOFTY_CAR_FINAL_DRIVE, "gearbox.final_drive_ratio", GEARBOX, 0x28, Kind::obf, 1, 20, Rederive::gearbox},
    // Ratios R, N, 1st..9th are CarObfFloat pairs at +0x38 stride 0x10: 1st is +0x58.
    {SWOFTY_CAR_GEAR_RATIO_1, "gearbox.gear_ratio_1", GEARBOX, 0x58, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 1, "gearbox.gear_ratio_2", GEARBOX, 0x68, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 2, "gearbox.gear_ratio_3", GEARBOX, 0x78, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 3, "gearbox.gear_ratio_4", GEARBOX, 0x88, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 4, "gearbox.gear_ratio_5", GEARBOX, 0x98, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 5, "gearbox.gear_ratio_6", GEARBOX, 0xa8, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 6, "gearbox.gear_ratio_7", GEARBOX, 0xb8, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 7, "gearbox.gear_ratio_8", GEARBOX, 0xc8, Kind::obf, 0.1f, 20, Rederive::gearbox},
    {SWOFTY_CAR_GEAR_RATIO_1 + 8, "gearbox.gear_ratio_9", GEARBOX, 0xd8, Kind::obf, 0.1f, 20, Rederive::gearbox},
};

inline const Field *field(uint32_t id)
{
    for (auto &f : FIELDS)
        if (f.id == id)
            return &f;
    return nullptr;
}

struct Env {
    std::function<bool(uintptr_t addr, void *out, size_t n)> read;
    std::function<bool(uintptr_t addr, const void *in, size_t n)> write;
    std::function<bool()> flow_thread;
    std::function<bool()> enabled;
    std::function<void(uintptr_t engine)> engine_derived;
    std::function<void(uintptr_t gearbox, uint32_t flags)> gearbox_reset;
    uintptr_t table = 0, count = 0;
};

template <class Tag> struct Ops {
    static inline Env env;
    static inline std::atomic<bool> bound{false};

    static void bind(Env e)
    {
        bound = false;
        env = std::move(e);
        bound = true;
    }

    template <class T> static bool rd(uintptr_t a, T &v) { return a && env.read && env.read(a, &v, sizeof v); }

    static int available() { return bound.load() && env.table && env.count && (!env.enabled || env.enabled()) ? 1 : 0; }

    static int cars(uint32_t &n)
    {
        uint8_t c = 0;
        if (!available() || !rd(env.count, c))
            return SWOFTY_ASSET_UNAVAILABLE;
        n = c < MAX_CARS ? c : MAX_CARS;
        return SWOFTY_ASSET_OK;
    }

    static int car_at(uint32_t index, const void *handle, uintptr_t &car)
    {
        uint32_t n = 0;
        if (int r = cars(n); r != SWOFTY_ASSET_OK)
            return r;
        if (index >= MAX_CARS)
            return SWOFTY_ASSET_INVALID;
        uint8_t id = 0;
        if (index >= n || !rd(env.table + index * 8, car) || !car || !rd(car + CAR_VEHICLE_ID, id) || id != index)
            return SWOFTY_ASSET_LATE;
        if (handle && car != reinterpret_cast<uintptr_t>(handle))
            return SWOFTY_ASSET_LATE;
        return SWOFTY_ASSET_OK;
    }

    static int system_at(uintptr_t car, const Field &f, uintptr_t &sys)
    {
        uintptr_t owner = 0;
        if (!rd(car + f.system, sys))
            return SWOFTY_ASSET_LATE;
        if (!sys)
            return SWOFTY_ASSET_UNAVAILABLE;
        return rd(sys + SYSTEM_CAR, owner) && owner == car ? SWOFTY_ASSET_OK : SWOFTY_ASSET_LATE;
    }

    static bool load(uintptr_t at, Kind kind, float &v)
    {
        uint32_t a = 0, key = 0;
        if (!rd(at, a) || (kind == Kind::obf && !rd(at + 8, key)))
            return false;
        a ^= key;
        std::memcpy(&v, &a, 4);
        return true;
    }

    static int op_available() { return available(); }

    static int op_count(uint32_t *n)
    {
        uint32_t c = 0;
        int r = cars(c);
        if (r == SWOFTY_ASSET_OK && n)
            *n = c;
        return r;
    }

    static int op_car(uint32_t index, void **handle, uint32_t *is_ai)
    {
        uintptr_t car = 0;
        if (int r = car_at(index, nullptr, car); r != SWOFTY_ASSET_OK)
            return r;
        uint32_t ai = 0;
        if (is_ai && !rd(car + CAR_IS_AI, ai))
            return SWOFTY_ASSET_LATE;
        if (handle)
            *handle = reinterpret_cast<void *>(car);
        if (is_ai)
            *is_ai = ai;
        return SWOFTY_ASSET_OK;
    }

    static int op_get(uint32_t index, const void *handle, uint32_t id, float *value)
    {
        const Field *f = field(id);
        if (!f || !value)
            return SWOFTY_ASSET_INVALID;
        uintptr_t car = 0, sys = 0;
        if (int r = car_at(index, handle, car); r != SWOFTY_ASSET_OK)
            return r;
        if (int r = system_at(car, *f, sys); r != SWOFTY_ASSET_OK)
            return r;
        return load(sys + f->offset, f->kind, *value) ? SWOFTY_ASSET_OK : SWOFTY_ASSET_LATE;
    }

    static int op_set(uint32_t index, const void *handle, uint32_t id, float value)
    {
        const Field *f = field(id);
        if (!f || !std::isfinite(value) || value < f->lo || value > f->hi)
            return SWOFTY_ASSET_INVALID;
        if (env.flow_thread && !env.flow_thread())
            return SWOFTY_ASSET_LATE;
        uintptr_t car = 0, sys = 0;
        if (int r = car_at(index, handle, car); r != SWOFTY_ASSET_OK)
            return r;
        if (int r = system_at(car, *f, sys); r != SWOFTY_ASSET_OK)
            return r;
        if ((f->rederive == Rederive::engine && !env.engine_derived) ||
            (f->rederive == Rederive::gearbox && !env.gearbox_reset))
            return SWOFTY_ASSET_UNAVAILABLE;
        // With setup ratios on, the gearbox builds its table from the setup and ignores the handling ratios.
        uint8_t setup_ratios = 0;
        if (f->rederive == Rederive::gearbox &&
            (!rd(sys + GEARBOX_USE_SETUP_RATIOS, setup_ratios) || setup_ratios))
            return SWOFTY_ASSET_UNAVAILABLE;
        if (id == SWOFTY_CAR_FUEL_MASS) {
            float capacity = 0;
            if (load(sys + FUEL_CAPACITY, Kind::obf, capacity) && capacity > 0 && value > capacity)
                return SWOFTY_ASSET_INVALID;
        }
        uint32_t bits = 0, key = 0;
        std::memcpy(&bits, &value, 4);
        if (f->kind == Kind::obf && !rd(sys + f->offset + 8, key))
            return SWOFTY_ASSET_LATE;
        bits ^= key;
        if (!env.write || !env.write(sys + f->offset, &bits, 4))
            return SWOFTY_ASSET_LATE;
        if (f->rederive == Rederive::engine)
            env.engine_derived(sys);
        else if (f->rederive == Rederive::gearbox)
            env.gearbox_reset(sys, GEARBOX_REBUILD);
        return SWOFTY_ASSET_OK;
    }

    // car.built and car.reset: the game builds each car on a loader thread (T119) and re-reads its parts through
    // Car_Systems_Reset_Thunk (T120), so the hooks only queue the car (site_Car_LiveCarBuilt: rdi = the new CarLive;
    // the thunk: its loader argument, CarLive+0x4a0) and the next frame takes the queue on the flow thread, where
    // set() may write, keeping the cars still in their slot.
    struct Pending {
        uint32_t index;
        uintptr_t car;
        bool reset;
    };
    static inline std::mutex pending_lock;
    static inline std::vector<Pending> pending;

    static void queue(uintptr_t car, bool reset)
    {
        uint8_t id = 0;
        if (!bound.load() || !car || !rd(car + CAR_VEHICLE_ID, id) || id >= MAX_CARS)
            return;
        std::lock_guard<std::mutex> g(pending_lock);
        if (pending.size() < MAX_CARS * 4)
            pending.push_back({id, car, reset});
    }

    static void built(uintptr_t car) { queue(car, false); }
    static void reset(uintptr_t loader) { queue(loader >= CAR_LOADER ? loader - CAR_LOADER : 0, true); }

    static std::vector<Pending> take()
    {
        std::vector<Pending> out;
        {
            std::lock_guard<std::mutex> g(pending_lock);
            out.swap(pending);
        }
        std::erase_if(out, [](const Pending &p) {
            uintptr_t car = 0;
            return car_at(p.index, reinterpret_cast<const void *>(p.car), car) != SWOFTY_ASSET_OK;
        });
        return out;
    }

    static inline const SwoftyCarOps table = {
        SWOFTY_CAR_OPS_VERSION, op_available, op_count, op_car, op_get, op_set,
    };
};

}
