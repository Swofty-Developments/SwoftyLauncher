#pragma once

#include <swofty/json.hpp>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

// The player's car setup as the game's setup screen shows it, read from the live car (re/db structs CarTuning.h,
// CarSetupValues.h, subsystems/car.md#tuning). Each slider's 0..1 fraction (step / steps) sits on the F1Tuning system (car
// +0x590); the UI value is min + step * (max - min) / steps with the min, max and steps of the game database's
// car_setup.sliders rows (SPECS below). Wings are the F1Aero levels, brake pressure and bias sit on the BrakeECU, fuel on
// the FuelTank, tyre pressures on the wheels. Verified against the stock F1 25 "balanced" preset by the first proof lap.
// The sampler path allocates nothing: a Setup is a plain array.
namespace runtime::lap_setup {

enum Key {
    FRONT_WING, REAR_WING, DIFF_ON, DIFF_OFF, FRONT_CAMBER, REAR_CAMBER, FRONT_TOE, REAR_TOE, FRONT_SUSPENSION,
    REAR_SUSPENSION, FRONT_ARB, REAR_ARB, FRONT_RIDE, REAR_RIDE, BRAKE_PRESSURE, BRAKE_BIAS, ENGINE_BRAKING, TYRE_FL,
    TYRE_FR, TYRE_RL, TYRE_RR, FUEL_LOAD, KEYS
};

constexpr const char *NAMES[KEYS] = {
    "front_wing", "rear_wing", "diff_on_throttle", "diff_off_throttle", "front_camber", "rear_camber", "front_toe",
    "rear_toe", "front_suspension", "rear_suspension", "front_anti_roll_bar", "rear_anti_roll_bar", "front_ride_height",
    "rear_ride_height", "brake_pressure", "brake_bias", "engine_braking", "tyre_pressure_fl", "tyre_pressure_fr",
    "tyre_pressure_rl", "tyre_pressure_rr", "fuel_load"};

// UI ranges from the game database (car_setup.sliders.slider_N: min, max, steps; stock F1 25 file). A slider's UI value is
// min + step * (max - min) / steps, with the step the game keeps as a fraction step / steps on the F1Tuning system.
struct Spec {
    double min, max;
    int steps;
};

constexpr Spec SPECS[KEYS] = {
    {0, 50, 50},      {0, 50, 50},     {10, 100, 18},   {10, 100, 18},   {-3.5, -2.5, 10}, {-2, -1, 10},
    {0, 0.2, 20},     {0.1, 0.25, 15}, {1, 41, 40},     {1, 41, 40},     {1, 21, 20},      {1, 21, 20},
    {15, 35, 20},     {40, 60, 20},    {80, 100, 20},   {50, 70, 20},    {0, 100, 10},     {22.5, 29.5, 70},
    {22.5, 29.5, 70}, {20.5, 26.5, 60}, {20.5, 26.5, 60}, {5, 110, 105}};

constexpr uint64_t CAR_FUEL = 0x540, CAR_BRAKE = 0x550, CAR_AERO = 0x570, CAR_SUSP = 0x580, CAR_TUNING = 0x590,
                   CAR_HANDLING = 0x9f40, WHEELS = 0x70;
constexpr uint64_t AERO_FRONT = 0x19b4, AERO_REAR = 0x19c0, BRAKE_PRESSURE_AT = 0x21dc, BRAKE_BIAS_AT = 0x21d8,
                   FUEL_AT = 0x100, WHEEL_PRESSURE = 0x64, WHEEL_PRESSURE2 = 0x68;

struct Slider {
    Key key;
    uint64_t frac;
};

constexpr Slider SLIDERS[] = {
    {DIFF_ON, 0x214},  {DIFF_OFF, 0x218},  {ENGINE_BRAKING, 0x220}, {FRONT_CAMBER, 0x228}, {REAR_CAMBER, 0x22c},
    {FRONT_TOE, 0x230}, {REAR_TOE, 0x234}, {FRONT_SUSPENSION, 0x238}, {REAR_SUSPENSION, 0x23c}, {FRONT_ARB, 0x250},
    {REAR_ARB, 0x254},
};
constexpr uint64_t RIDE_FRONT_NEW = 0x248, RIDE_REAR_NEW = 0x24c, RIDE_FRONT_OLD = 0x240, RIDE_REAR_OLD = 0x244;

struct Setup {
    bool has[KEYS] = {};
    double v[KEYS] = {};
    double raw[KEYS] = {};
    int count = 0;
    bool changed = false;
    char note[160] = {};
};

inline bool finite(double d)
{
    return std::isfinite(d);
}

inline double lerp(double a, double b, double t)
{
    return a + (b - a) * t;
}

inline void put(Setup &s, Key k, double ui, double raw)
{
    if (!finite(ui) || ui < SPECS[k].min - 0.5 || ui > SPECS[k].max + 0.5)
        return;
    s.has[k] = true;
    s.v[k] = std::round(ui * 1000.0) / 1000.0;
    s.raw[k] = raw;
    s.count++;
}

inline double ui_of(Key k, double fraction)
{
    const Spec &p = SPECS[k];
    double t = fraction < 0 ? 0 : fraction > 1 ? 1 : fraction;
    return p.min + std::round(t * p.steps) * (p.max - p.min) / p.steps;
}

// Pressure units: Pa, kPa or psi, told apart by magnitude; the UI shows psi.
inline double psi_of(double v)
{
    if (v > 20000)
        return v * 0.00014503773773;
    if (v > 100)
        return v * 0.14503773773;
    if (v < 5)
        return v * 14.503773773;
    return v;
}

template <class R, class T> inline bool rd(R &mem, uint64_t at, T &out)
{
    return mem(at, &out, sizeof out);
}

template <class R> inline bool read(R &mem, uint64_t car, Setup &out)
{
    out = Setup{};
    uint64_t tune = 0, aero = 0, brake = 0, fuel = 0, susp = 0;
    uint32_t handling = 8;
    rd(mem, car + CAR_HANDLING, handling);
    if (!car || !rd(mem, car + CAR_TUNING, tune) || !tune)
        return false;
    rd(mem, car + CAR_AERO, aero);
    rd(mem, car + CAR_BRAKE, brake);
    rd(mem, car + CAR_FUEL, fuel);
    rd(mem, car + CAR_SUSP, susp);
    uint32_t lv = 0;
    if (aero && rd(mem, aero + AERO_FRONT, lv))
        put(out, FRONT_WING, lv, lv);
    if (aero && rd(mem, aero + AERO_REAR, lv))
        put(out, REAR_WING, lv, lv);
    for (const Slider &s : SLIDERS) {
        float f = 0;
        if (rd(mem, tune + s.frac, f) && finite(f) && f >= -0.001f && f <= 1.001f)
            put(out, s.key, ui_of(s.key, f), f);
    }
    bool modern = handling >= 7;
    float f = 0;
    if (rd(mem, tune + (modern ? RIDE_FRONT_NEW : RIDE_FRONT_OLD), f) && finite(f) && f >= 0 && f <= 1)
        put(out, FRONT_RIDE, ui_of(FRONT_RIDE, f), f);
    if (rd(mem, tune + (modern ? RIDE_REAR_NEW : RIDE_REAR_OLD), f) && finite(f) && f >= 0 && f <= 1)
        put(out, REAR_RIDE, ui_of(REAR_RIDE, f), f);
    float b = 0;
    if (brake && rd(mem, brake + BRAKE_PRESSURE_AT, b) && finite(b))
        put(out, BRAKE_PRESSURE, b <= 2.0f ? std::round(b * 100.0) : b, b);
    if (brake && rd(mem, brake + BRAKE_BIAS_AT, b) && finite(b))
        put(out, BRAKE_BIAS, ui_of(BRAKE_BIAS, b), b);
    float kg = 0;
    if (fuel && rd(mem, fuel + FUEL_AT, kg) && finite(kg))
        put(out, FUEL_LOAD, kg, kg);
    static const Key wheel_keys[4] = {TYRE_RL, TYRE_RR, TYRE_FL, TYRE_FR};
    int at = 0;
    for (int i = 0; i < 4; i++) {
        uint64_t wheel = 0;
        float p = 0, q = 0;
        bool read_ok = susp && rd(mem, susp + WHEELS + 8 * uint64_t(i), wheel) && wheel && rd(mem, wheel + WHEEL_PRESSURE, p) &&
                       finite(p);
        rd(mem, wheel + WHEEL_PRESSURE2, q);
        if (read_ok) {
            double psi = std::round(psi_of(p) * 10.0) / 10.0;
            put(out, wheel_keys[i], psi, p);
        }
        if (!out.has[wheel_keys[i]] && at < int(sizeof out.note) - 40)
            at += snprintf(out.note + at, sizeof out.note - size_t(at), "wheel%d ptr 0x%llx +0x64=%g +0x68=%g; ", i,
                           (unsigned long long)wheel, double(p), double(q));
    }
    return out.count >= 4;
}

inline bool differs(const Setup &a, const Setup &b)
{
    for (int k = 0; k < KEYS; k++)
        if (a.has[k] != b.has[k] || (a.has[k] && std::fabs(a.v[k] - b.v[k]) > 0.0005))
            return true;
    return false;
}

inline swofty::json::Value to_json(const Setup &s)
{
    swofty::json::Value o = swofty::json::Value::object();
    if (s.count < 4)
        return o;
    for (int k = 0; k < KEYS; k++) {
        if (!s.has[k])
            continue;
        double r = s.v[k];
        if (r == std::floor(r) && std::fabs(r) < 1e9)
            o[NAMES[k]] = int64_t(r);
        else
            o[NAMES[k]] = r;
    }
    o["changed"] = s.changed;
    return o;
}

inline std::string diag_text(const Setup &s)
{
    std::string out;
    char b[96];
    for (int k = 0; k < KEYS; k++) {
        if (!s.has[k])
            continue;
        snprintf(b, sizeof b, "%s%s=%.3f(raw %.4f)", out.empty() ? "" : " ", NAMES[k], s.v[k], s.raw[k]);
        out += b;
    }
    if (s.note[0])
        out += std::string(" [tyre pressures unreadable: ") + s.note + "]";
    return out.empty() ? "unreadable" : out;
}

}
