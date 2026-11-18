// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#include <optional>
#include <swofty/live.hpp>

// The pit lane: each car's garage and pit box, from the core's live state (swofty/live.hpp). The core reads the garage
// index Game_PitLane_AssignGarageIndices gives each car when a session's pit lane is built: 2 x the team's garage slot
// plus the car's seat (the reverse on tracks whose garage order runs the other way), 99 for the safety car. The pit
// box is the garage index / 2, box 12 for the safety car. Both need the live state (the experimental feature
// telemetry) on a build where its pit lane group is on, and a game header that exports the garage field; elsewhere
// they read nothing. A car's place in the pit lane and its stops are live::pit_status and live::pit_stops, and the
// moments it changes are session.pit and session.pit_stop (swofty/systems/sessions.hpp).

namespace swofty::pitlane {

inline constexpr uint8_t safety_car_garage = SWOFTY_LIVE_GARAGE_SAFETY_CAR;
inline constexpr uint8_t safety_car_box = 12;

inline std::optional<uint8_t> garage(const live::Car &c)
{
    return live::has(c, SWOFTY_LIVE_HAVE_GARAGE) ? std::optional<uint8_t>(c.garage) : std::nullopt;
}

inline std::optional<uint8_t> pit_box(const live::Car &c)
{
    return live::has(c, SWOFTY_LIVE_HAVE_GARAGE) ? std::optional<uint8_t>(c.pit_box) : std::nullopt;
}

inline bool safety_car(const live::Car &c)
{
    return live::has(c, SWOFTY_LIVE_HAVE_GARAGE) && c.garage == safety_car_garage;
}

// The car's seat in its team's garage slot: 0 or 1, the lower bit of the garage index. On a track whose garage order
// runs the other way the game swaps the two seats, so this is the side of the slot, not the driver's number.
inline std::optional<uint8_t> side(const live::Car &c)
{
    if (!live::has(c, SWOFTY_LIVE_HAVE_GARAGE) || c.garage == safety_car_garage)
        return std::nullopt;
    return uint8_t(c.garage & 1);
}

}
