#pragma once

#include <swofty/json.hpp>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

// The player's car setup as the game's setup screen shows it, read from the live car (re/db structs CarTuning.h,
// CarSetupValues.h, subsystems/car.md#tuning). Each slider's 0..1 fraction sits on the F1Tuning system (car +0x590) and is
// lerped over the Min/Max pair the same system carries; wings are the F1Aero levels; brake bias and pressure sit on the
// BrakeECU, fuel on the FuelTank, tyre pressures on the wheels. The conversions to the UI numbers are static readings of the
// game's own formulas and heuristics for the units (marked below); the [laps] setup diag line prints the raw inputs so one
// lap confirms or corrects them. The sampler path allocates nothing: a Setup is a plain array.
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

// Plausible UI ranges; a value outside drops that key.
constexpr double LOW[KEYS] = {0, 0, 0, 0, -10, -10, -2, -2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5, 0};
constexpr double HIGH[KEYS] = {100, 100, 100, 100, 5, 5, 2, 2, 100, 100, 100, 100, 200, 200, 100, 100, 100, 50, 50, 50, 50, 200};

constexpr uint64_t CAR_FUEL = 0x540, CAR_BRAKE = 0x550, CAR_AERO = 0x570, CAR_SUSP = 0x580, CAR_TUNING = 0x590,
                   CAR_HANDLING = 0x9f40, WHEELS = 0x70;
constexpr uint64_t AERO_FRONT = 0x19b4, AERO_REAR = 0x19c0, BRAKE_PRESSURE_AT = 0x21dc, BRAKE_BIAS_AT = 0x21d8,
                   FUEL_AT = 0x100, WHEEL_PRESSURE = 0x64;

struct Slider {
    Key key;
    uint64_t frac, lo, hi;
};

constexpr Slider SLIDERS[] = {
    {DIFF_ON, 0x214, 0x30, 0x34},       {DIFF_OFF, 0x218, 0x38, 0x3c},      {ENGINE_BRAKING, 0x220, 0x58, 0x5c},
    {FRONT_CAMBER, 0x228, 0x68, 0x6c},  {REAR_CAMBER, 0x22c, 0x70, 0x74},   {FRONT_TOE, 0x230, 0x78, 0x7c},
    {REAR_TOE, 0x234, 0x80, 0x84},      {FRONT_SUSPENSION, 0x238, 0, 0},     {REAR_SUSPENSION, 0x23c, 0, 0},
    {FRONT_ARB, 0x250, 0, 0},           {REAR_ARB, 0x254, 0, 0},
};
constexpr int SUSPENSION_STEPS = 40, ARB_STEPS = 20;
constexpr uint64_t RIDE_FRONT_NEW = 0x248, RIDE_REAR_NEW = 0x24c, RIDE_FRONT_OLD = 0x240, RIDE_REAR_OLD = 0x244,
                   RIDE_LO_F = 0xa8, RIDE_HI_F = 0xac, RIDE_LO_R = 0xb0, RIDE_HI_R = 0xb4;
constexpr uint64_t BRAKE_P_LO = 0xe8, BRAKE_P_HI = 0xec;

struct Setup {
    bool has[KEYS] = {};
    double v[KEYS] = {};
    double raw[KEYS] = {};
    int count = 0;
    bool changed = false;
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
    if (!finite(ui) || ui < LOW[k] || ui > HIGH[k])
        return;
    s.has[k] = true;
    s.v[k] = std::round(ui * 1000.0) / 1000.0;
    s.raw[k] = raw;
    s.count++;
}

// Angles in the F1Tuning data are radians when the range is tiny; the UI shows degrees.
inline double angle_ui(double v, double hi)
{
    return std::fabs(hi) <= 0.2 ? v * 57.29577951308232 : v;
}

// Pressure units: Pa, kPa or psi, told apart by magnitude; the UI shows psi.
inline double psi_of(double v)
{
    if (v > 20000)
        return v * 0.00014503773773;
    if (v > 100)
        return v * 0.14503773773;
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
        float f = 0, lo = 0, hi = 0;
        if (!rd(mem, tune + s.frac, f) || !finite(f) || f < -0.001f || f > 1.001f)
            continue;
        double t = f < 0 ? 0 : f > 1 ? 1 : f;
        if (s.key == FRONT_SUSPENSION || s.key == REAR_SUSPENSION) {
            put(out, s.key, std::round(t * SUSPENSION_STEPS) + 1, f);
        } else if (s.key == FRONT_ARB || s.key == REAR_ARB) {
            put(out, s.key, std::round(t * ARB_STEPS) + 1, f);
        } else if (rd(mem, tune + s.lo, lo) && rd(mem, tune + s.hi, hi) && finite(lo) && finite(hi)) {
            double ui = lerp(lo, hi, t);
            if (s.key == FRONT_CAMBER || s.key == REAR_CAMBER || s.key == FRONT_TOE || s.key == REAR_TOE)
                ui = angle_ui(ui, hi);
            else if (hi <= 1.5)
                ui *= 100.0;
            put(out, s.key, ui, f);
        }
    }
    bool modern = handling >= 7;
    float f = 0, lo = 0, hi = 0;
    if (rd(mem, tune + (modern ? RIDE_FRONT_NEW : RIDE_FRONT_OLD), f) && rd(mem, tune + RIDE_LO_F, lo) &&
        rd(mem, tune + RIDE_HI_F, hi) && finite(f) && f >= 0 && f <= 1)
        put(out, FRONT_RIDE, hi <= 1.0 ? lerp(lo, hi, f) * 1000.0 : lerp(lo, hi, f), f);
    if (rd(mem, tune + (modern ? RIDE_REAR_NEW : RIDE_REAR_OLD), f) && rd(mem, tune + RIDE_LO_R, lo) &&
        rd(mem, tune + RIDE_HI_R, hi) && finite(f) && f >= 0 && f <= 1)
        put(out, REAR_RIDE, hi <= 1.0 ? lerp(lo, hi, f) * 1000.0 : lerp(lo, hi, f), f);
    float b = 0;
    if (brake && rd(mem, brake + BRAKE_PRESSURE_AT, b) && finite(b))
        put(out, BRAKE_PRESSURE, b <= 2.0f ? b * 100.0 : b, b);
    if (brake && rd(mem, brake + BRAKE_BIAS_AT, b) && finite(b))
        put(out, BRAKE_BIAS, b <= 1.5f ? b * 100.0 : b, b);
    float kg = 0;
    if (fuel && rd(mem, fuel + FUEL_AT, kg) && finite(kg))
        put(out, FUEL_LOAD, kg, kg);
    static const Key wheel_keys[4] = {TYRE_RL, TYRE_RR, TYRE_FL, TYRE_FR};
    for (int i = 0; i < 4 && susp; i++) {
        uint64_t wheel = 0;
        float p = 0;
        if (rd(mem, susp + WHEELS + 8 * uint64_t(i), wheel) && wheel && rd(mem, wheel + WHEEL_PRESSURE, p) && finite(p))
            put(out, wheel_keys[i], psi_of(p), p);
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
    return out.empty() ? "unreadable" : out;
}

}
