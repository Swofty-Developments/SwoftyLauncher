// SPDX-License-Identifier: MIT
#pragma once
#include <optional>
#include <vector>
#include <swofty/events.hpp>
#include <swofty/registry.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/control.hpp>
#include <swofty/systems/forward.hpp>

// Sessions: in 1.6.0 the rules a Grand Prix or Time Trial box carries (content::Package::rules), applied
// when a session loads in that box, and who drives a car: set_control hands a car to the AI or back to its
// human, and session.player.control runs around the game's own switches; session.lap fires when a car
// crosses the line. The rest of the sessions system ships later.

namespace swofty::sessions {

// A points system (launcher 1.9.49): one game_setup.session_standings row the core adds at boot, a copy of the stock
// standard_race preset with these points. A box names it in content.package rules.points; the era picker points the
// race sessions of swofty_f1 at it when the box is pressed.
struct Points {
    static constexpr Version version{1, 0};
    std::vector<int> points;                   // per finishing position, from P1; padded with 0 to 26 places
    std::optional<int> fastest_lap;            // the fastest lap bonus (BestOfTop10); empty: none
    std::vector<std::vector<int>> reduced;     // races stopped at 75, 50 and 25 % distance; a missing entry: the full points
    json::Value extra;
};
SWOFTY_RECORD(Points, points, fastest_lap, reduced)
inline Registry<Points> points{"session.points", {.applies = Applies::boot}};

struct Rules {
    Ref<Mode> mode;
    Ref<Points> points;
    Ref<power::DrsRule> drs;
    Ref<tyres::Class> tyre_class;
    Ref<RulesProfile> rules_profile;
    Ref<cars::Profile> car_profile;
    bool refuelling = false;
    std::optional<float> race_fuel_kg;
    std::optional<bool> tyre_warmers;   // content.package 1.4: the box's races with or without tyre warmers; empty = the mode's
};
SWOFTY_RECORD(Rules, mode, points, drs, tyre_class, rules_profile, car_profile, refuelling, race_fuel_kg, tyre_warmers)

inline Result set_control(cars::live::Car car, Control mode)
{
    auto t = swofty::detail::live_session_ops();
    if (!t || !t->set_control)
        return Result::unavailable;
    return swofty::detail::session_result(t->set_control(car.index(), mode == Control::ai ? 1u : 0u));
}

// Disqualifies a car through the game's own rules (standard library 1.8, experimental): the car goes to the AI
// with the DISQUALIFIED reason and the session result marks it disqualified. late before the session has a rule
// manager or while the car has no controller set, unavailable on a core without session operations version 2 or
// when the game call does not resolve. Called while the game swaps controllers it is queued to the next frame.
inline Result disqualify(cars::live::Car car)
{
    auto t = swofty::detail::live_session_ops();
    if (!t || t->version < 2 || !t->disqualify)
        return Result::unavailable;
    return swofty::detail::session_result(t->disqualify(car.index()));
}

namespace events {

struct PlayerControl {
    static constexpr const char *point = "session.player.control";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    cars::live::Car car;
    Control from, to;
    int32_t reason;
};

// A car crosses the line and its lap counter changes (notify). It fires once per crossing for each car the
// session's ERS system tracks, on the game thread, inside the game's per-car progress update: a handler
// that wants to change the session queues the work for a later frame. The counter also changes when a car
// crosses the line backwards, and a crossing in the pit lane fires too.
struct Lap {
    static constexpr const char *point = "session.lap";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    cars::live::Car car;
};

// A car's pit status changes (notify): inside Game_Pitlane_SetVehicleStatus, on the game thread, after the game has
// stored the new status. status is the game's pit status number (0 to 16) and previous the one it replaced.
struct Pit {
    static constexpr const char *point = "session.pit";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    uint32_t previous;
    uint32_t status;
};

// A car's pit stop is counted (notify), on the status change where the game adds one to the car's stop count.
// stops is the count after it.
struct PitStop {
    static constexpr const char *point = "session.pit_stop";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    uint32_t stops;
};

}

}
