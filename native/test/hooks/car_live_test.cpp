// Host unit test for the core's live car operations (car_live.hpp) and the SDK side of cars::live
// (cars.hpp): the operations run on fake cars laid out as the KB's CarLive and its systems
// (Steering, GearBox, Differential, Turbo, FuelTank, F1WingDamage, F1SuspensionDamage) with fresh XOR keys
// on the obfuscated values. It checks that nothing is answered before the table is bound or without the
// globals, the car list and the slot/owner/handle checks, decoding and re-encoding (the key is kept, the
// plain half changes), range refusals (steering lock, differential, fuel over capacity, NaN), writes off
// the flow thread, a car without a system, and cars::live::cars/get/set/scale/is_ai through the table.
// run_car_live_test.sh builds and runs it.
#include "../../src/core/hooks/car_live.hpp"

#include <swofty/systems/cars.hpp>

#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>

namespace cl = swofty_core::hooks::car_live;
namespace live = swofty::cars::live;
using swofty::Result;

static int fails;
static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Tag {};
using Ops = cl::Ops<Tag>;

static constexpr int CARS = 3;
alignas(16) static uint8_t car_mem[CARS][0x600];
alignas(16) static uint8_t steering[CARS][0x100], gearbox[CARS][0x700], diff[CARS][0x100], turbo[CARS][0x310],
    tank[CARS][0x110], wing[CARS][0xc8], susp[CARS][0x140];
static uintptr_t table[32];
static uint32_t count;
static bool flow = true;

template <class T> static T &at(uint8_t *base, size_t off) { return *reinterpret_cast<T *>(base + off); }

static uint32_t bits(float f)
{
    uint32_t b;
    memcpy(&b, &f, 4);
    return b;
}

static void put_obf(uint8_t *base, size_t off, float v, uint32_t key)
{
    at<uint32_t>(base, off + 8) = key;
    at<uint32_t>(base, off) = bits(v) ^ key;
}

static float plain(uint8_t *base, size_t off) { return at<float>(base, off); }

static void attach(uint8_t *car, uintptr_t slot, uint8_t *sys)
{
    at<void *>(car, slot) = sys;
    at<void *>(sys, cl::SYSTEM_CAR) = car;
}

static void build_world()
{
    memset(car_mem, 0, sizeof car_mem);
    memset(table, 0, sizeof table);
    for (int i = 0; i < CARS; i++) {
        uint8_t *c = car_mem[i];
        table[i] = reinterpret_cast<uintptr_t>(c);
        c[cl::CAR_VEHICLE_ID] = uint8_t(i);
        at<uint32_t>(c, cl::CAR_IS_AI) = i == 0 ? 0 : 1;
        attach(c, cl::STEERING, steering[i]);
        attach(c, cl::GEARBOX, gearbox[i]);
        attach(c, cl::DIFFERENTIAL, diff[i]);
        attach(c, cl::FUEL_TANK, tank[i]);
        attach(c, cl::WING_DAMAGE, wing[i]);
        attach(c, cl::SUSPENSION_DAMAGE, susp[i]);
        if (i != 2)
            attach(c, cl::TURBO, turbo[i]);
        put_obf(steering[i], 0x28, 0.34034f, 0xa5a50000u + i);
        at<float>(gearbox[i], 0xe8) = 1.5f;
        at<float>(diff[i], 0xf0) = 0.5f;
        at<float>(diff[i], 0xec) = 0.1f;
        at<float>(turbo[i], 0x2d8) = 1.0f;
        put_obf(tank[i], cl::FUEL_CAPACITY, 110.0f, 0x13572468u);
        at<float>(tank[i], 0x100) = 100.0f;
        at<float>(wing[i], 0x28) = 3.0f;
        at<float>(wing[i], 0x2c) = 0.0001f;
        at<float>(wing[i], 0x38) = 1.0f;
        put_obf(susp[i], 0x28, 7.0f, 0x0badf00du);
        put_obf(susp[i], 0x38, 0.001f, 0xdeadbeefu);
        put_obf(susp[i], 0x48, 0.0001f, 0x01020304u);
        put_obf(susp[i], 0x58, 1.0f, 0x55aa55aau);
        put_obf(susp[i], 0x68, 1.5f, 0xfeedfaceu);
        at<float>(susp[i], 0x78) = 50.0f;
        at<float>(susp[i], 0x7c) = 30.0f;
    }
    count = CARS;
}

static cl::Env make_env(bool globals = true)
{
    cl::Env e;
    e.read = [](uintptr_t a, void *out, size_t n) {
        memcpy(out, reinterpret_cast<const void *>(a), n);
        return true;
    };
    e.write = [](uintptr_t a, const void *in, size_t n) {
        memcpy(reinterpret_cast<void *>(a), in, n);
        return true;
    };
    e.flow_thread = [] { return flow; };
    e.enabled = [] { return true; };
    if (globals) {
        e.table = reinterpret_cast<uintptr_t>(table);
        e.count = reinterpret_cast<uintptr_t>(&count);
    }
    return e;
}

static void core_ops()
{
    build_world();
    float v = 0;
    uint32_t n = 0;
    expect(!Ops::op_available() && Ops::op_count(&n) == SWOFTY_ASSET_UNAVAILABLE, "nothing before the table is bound");
    Ops::bind(make_env(false));
    expect(!Ops::op_available() && Ops::op_get(0, nullptr, SWOFTY_CAR_STEERING_LOCK, &v) == SWOFTY_ASSET_UNAVAILABLE,
           "unavailable when the vehicle table does not resolve");
    Ops::bind(make_env());
    expect(Ops::op_available() == 1, "available once bound");
    expect(Ops::op_count(&n) == SWOFTY_ASSET_OK && n == 3, "count reads g_Sess_VehicleCount");

    void *h = nullptr;
    uint32_t ai = 9;
    expect(Ops::op_car(0, &h, &ai) == SWOFTY_ASSET_OK && h == car_mem[0] && ai == 0, "car 0: handle and is_ai 0");
    expect(Ops::op_car(1, &h, &ai) == SWOFTY_ASSET_OK && h == car_mem[1] && ai == 1, "car 1: is_ai 1");
    expect(Ops::op_car(3, &h, nullptr) == SWOFTY_ASSET_LATE, "an index past the count is late");
    expect(Ops::op_car(40, &h, nullptr) == SWOFTY_ASSET_INVALID, "an index past 31 is invalid");

    expect(Ops::op_get(0, nullptr, SWOFTY_CAR_STEERING_LOCK, &v) == SWOFTY_ASSET_OK && std::fabs(v - 0.34034f) < 1e-6f,
           "steering lock decodes the XOR pair");
    expect(Ops::op_get(1, car_mem[1], SWOFTY_CAR_SUSP_FRONT_DAMAGE_SCALE, &v) == SWOFTY_ASSET_OK &&
               std::fabs(v - 0.001f) < 1e-9f,
           "suspension front damage scale decodes with the handle checked");
    expect(Ops::op_get(0, nullptr, SWOFTY_CAR_CLUTCH_TORQUE, &v) == SWOFTY_ASSET_OK && v == 1.5f, "clutch torque plain");
    expect(Ops::op_get(0, nullptr, 99, &v) == SWOFTY_ASSET_INVALID, "an unknown field is invalid");
    expect(Ops::op_get(0, car_mem[1], SWOFTY_CAR_CLUTCH_TORQUE, &v) == SWOFTY_ASSET_LATE,
           "a handle that isn't the slot's car is late");
    expect(Ops::op_get(2, nullptr, SWOFTY_CAR_TURBO_MAX_BOOST, &v) == SWOFTY_ASSET_UNAVAILABLE,
           "a car without the system answers unavailable");

    uint32_t key = at<uint32_t>(steering[0], 0x30);
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_STEERING_LOCK, 0.4f) == SWOFTY_ASSET_OK, "set steering lock");
    expect(at<uint32_t>(steering[0], 0x30) == key && (at<uint32_t>(steering[0], 0x28) ^ key) == bits(0.4f),
           "the obfuscated value is re-encoded under its key");
    expect(Ops::op_get(0, nullptr, SWOFTY_CAR_STEERING_LOCK, &v) == SWOFTY_ASSET_OK && v == 0.4f, "and reads back");
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_STEERING_LOCK, 2.0f) == SWOFTY_ASSET_INVALID &&
               Ops::op_set(0, nullptr, SWOFTY_CAR_STEERING_LOCK, 0.0f) == SWOFTY_ASSET_INVALID,
           "steering lock outside 0.01 .. pi/2 is refused");
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_DIFF_ON_THROTTLE, 1.2f) == SWOFTY_ASSET_INVALID &&
               plain(diff[0], 0xf0) == 0.5f,
           "differential above 1 is refused and untouched");
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_DIFF_OFF_THROTTLE, 0.3f) == SWOFTY_ASSET_OK && plain(diff[0], 0xec) == 0.3f,
           "differential off throttle written plain");
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_WING_DAMAGE_SCALE, NAN) == SWOFTY_ASSET_INVALID, "NaN is refused");
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_FUEL_MASS, 120.0f) == SWOFTY_ASSET_INVALID &&
               plain(tank[0], 0x100) == 100.0f,
           "fuel over the tank's capacity is refused");
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_FUEL_MASS, 50.0f) == SWOFTY_ASSET_OK && plain(tank[0], 0x100) == 50.0f,
           "fuel within capacity is written");
    flow = false;
    expect(Ops::op_set(0, nullptr, SWOFTY_CAR_CLUTCH_TORQUE, 2.0f) == SWOFTY_ASSET_LATE && plain(gearbox[0], 0xe8) == 1.5f,
           "a write off the flow thread is late and changes nothing");
    flow = true;

    at<void *>(susp[1], cl::SYSTEM_CAR) = car_mem[0];
    expect(Ops::op_set(1, nullptr, SWOFTY_CAR_SUSP_SPEED_THRESHOLD, 9.0f) == SWOFTY_ASSET_LATE,
           "a system whose owner is another car is late");
    at<void *>(susp[1], cl::SYSTEM_CAR) = car_mem[1];
    car_mem[1][cl::CAR_VEHICLE_ID] = 2;
    expect(Ops::op_car(1, &h, nullptr) == SWOFTY_ASSET_LATE, "a slot whose car has another vehicle id is late");
    car_mem[1][cl::CAR_VEHICLE_ID] = 1;
    table[1] = 0;
    expect(Ops::op_car(1, &h, nullptr) == SWOFTY_ASSET_LATE, "an empty slot is late");
    table[1] = reinterpret_cast<uintptr_t>(car_mem[1]);
}

static void sdk_side()
{
    build_world();
    Ops::bind(make_env());
    live::Car none(0);
    expect(!live::available() && live::cars().empty() && !none.get(live::Field::steering_lock) &&
               none.set(live::Field::steering_lock, 0.3f) == Result::unavailable && none.is_ai(),
           "the SDK answers nothing without the table");
    swofty::detail::set_car_ops_table(&Ops::table);
    expect(live::available(), "available through the table");
    auto all = live::cars();
    expect(all.size() == 3 && all[1].index() == 1 && all[1].handle() == car_mem[1], "cars() lists index and handle");
    expect(!all[0].is_ai() && all[1].is_ai(), "is_ai reads the car");
    auto lock = all[0].get(live::Field::steering_lock);
    expect(lock && std::fabs(*lock - 0.34034f) < 1e-6f, "get decodes");
    expect(all[0].scale(live::Field::steering_lock, 1.1f) == Result::ok && all[0].get(live::Field::steering_lock) &&
               std::fabs(*all[0].get(live::Field::steering_lock) - 0.374374f) < 1e-5f,
           "scale reads, multiplies and writes");
    expect(all[1].set(live::Field::susp_wheel_detach_scale, 0.8f) == Result::ok &&
               all[1].get(live::Field::susp_wheel_detach_scale) == 0.8f,
           "set re-encodes a suspension value");
    expect(all[1].set(live::Field::diff_on_throttle, -0.1f) == Result::invalid, "set reports a refused value");
    expect(all[2].scale(live::Field::turbo_max_boost, 2.0f) == Result::unavailable, "scale passes unavailable on");
    table[1] = reinterpret_cast<uintptr_t>(car_mem[2]);
    expect(!all[1].get(live::Field::clutch_torque) && all[1].is_ai(), "a stale handle reads nothing");
    swofty::detail::set_car_ops_table(nullptr);
}

int main()
{
    core_ops();
    sdk_side();
    if (fails) {
        printf("car_live_test: %d failure(s)\n", fails);
        return 1;
    }
    printf("car_live_test: ok\n");
    return 0;
}
