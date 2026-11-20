// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#include <optional>
#include <swofty/events.hpp>
#include <swofty/live.hpp>

// Tyre damage: each wheel's puncture state and deflation, from the core's live state (swofty/live.hpp). The game marks
// a wheel punctured when its tyre damage monitor adds a puncture (CarWheel.tyre_flags bit 1) and burst once the
// deflation reaches 2 (bit 0, set only while bit 1 is); a tyre change clears both. The core reads the four wheels of
// every car once per live tick and fires car.tyre.damage when a wheel's state changes. Everything needs the live state
// (the experimental feature telemetry) on a build where its tyre damage read is proven (the KB build and Steam
// 1.0.145); elsewhere the reads give nothing and the event does not fire.

namespace swofty::tyre_damage {

enum class State : uint8_t {
    none = 0,
    punctured = SWOFTY_LIVE_TYRE_PUNCTURED,
    burst = SWOFTY_LIVE_TYRE_BURST,
    unknown = 0xff,
};

// The order of SwoftyLiveCar.tyre_damage and tyre_deflation.
enum class Wheel : uint8_t { rear_left = 0, rear_right = 1, front_left = 2, front_right = 3 };

inline constexpr unsigned WHEELS = 4;

inline const char *name(State s)
{
    switch (s) {
    case State::none:
        return "none";
    case State::punctured:
        return "punctured";
    case State::burst:
        return "burst";
    default:
        return "unknown";
    }
}

inline const char *name(Wheel w)
{
    static const char *const names[] = {"RL", "RR", "FL", "FR"};
    return unsigned(w) < WHEELS ? names[unsigned(w)] : "?";
}

inline std::optional<State> state(const live::Car &c, Wheel w)
{
    if (!live::has(c, SWOFTY_LIVE_HAVE_TYRE_DAMAGE) || unsigned(w) >= WHEELS)
        return std::nullopt;
    uint8_t v = c.tyre_damage[unsigned(w)];
    return v == SWOFTY_LIVE_TYRE_PUNCTURED || v == SWOFTY_LIVE_TYRE_BURST ? State(v) : State::none;
}

inline std::optional<float> deflation(const live::Car &c, Wheel w)
{
    if (!live::has(c, SWOFTY_LIVE_HAVE_TYRE_DAMAGE) || unsigned(w) >= WHEELS)
        return std::nullopt;
    return c.tyre_deflation[unsigned(w)];
}

// The car's punctured or burst wheels, bit i for Wheel(i); empty without the read.
inline std::optional<uint8_t> damaged(const live::Car &c)
{
    if (!live::has(c, SWOFTY_LIVE_HAVE_TYRE_DAMAGE))
        return std::nullopt;
    uint8_t out = 0;
    for (unsigned i = 0; i < WHEELS; i++)
        if (c.tyre_damage[i] == SWOFTY_LIVE_TYRE_PUNCTURED || c.tyre_damage[i] == SWOFTY_LIVE_TYRE_BURST)
            out = uint8_t(out | (1u << i));
    return out;
}

namespace events {

// A wheel's puncture state changes (notify, flow thread): punctured, burst, or back to none after a tyre change. car is
// the vehicle id, lap the car's lap, deflation the wheel's deflation level in the tick the change was read.
struct Damage {
    static constexpr const char *point = "car.tyre.damage";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    Wheel wheel;
    State state;
    State previous;
    uint8_t reserved;
    float deflation;
    int32_t lap;
};

}

}
