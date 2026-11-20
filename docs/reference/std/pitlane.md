---
title: Pit lane
description: Each car's garage and pit box from swofty/systems/pitlane.hpp, read from the live state.
---

# Pit lane

The pit lane library gives each car's garage and pit box. The core reads the garage index the game assigns when a session's pit lane is built, through the [live state](./live). A pit board or a garage HUD reads which box a car stops in without reading memory itself.

```cpp [native/src/pit_board.cpp]
#include <swofty/live.hpp>
#include <swofty/systems/pitlane.hpp>

namespace live = swofty::live;
namespace pit = swofty::pitlane;

void draw_pit_board()
{
    std::optional<live::Car> me = live::player();
    if (!me)
        return;
    if (auto box = pit::pit_box(*me))
        swofty::log("[pit-board] box %u, side %u", unsigned(*box), unsigned(pit::side(*me).value_or(0)));
    for (uint32_t v = 0; v < SWOFTY_LIVE_MAX_CARS; v++)
        if (auto c = live::car(v); c && pit::safety_car(*c))
            swofty::log("[pit-board] safety car (vehicle %u) in box %u", v, unsigned(pit::safety_car_box));
}
```

```cpp
namespace swofty::pitlane {

inline constexpr uint8_t safety_car_garage = 99;
inline constexpr uint8_t safety_car_box = 12;

std::optional<uint8_t> garage(const live::Car &c);   // 2 x the team's garage slot + the car's side; 99 the safety car
std::optional<uint8_t> pit_box(const live::Car &c);  // garage / 2; 12 for the safety car
std::optional<uint8_t> side(const live::Car &c);     // 0 or 1, the lower bit of the garage; none for the safety car
bool safety_car(const live::Car &c);

}
```

- **The garage index** is the one `Game_PitLane_AssignGarageIndices` writes when the pit lane is built: 2 x the team's garage slot plus the car's seat, or 2 x the slot + 1 - the seat on a track whose garage order runs the other way. The game puts a car in pit box garage / 2 (`Game_PitLane_PutVehicleInPitBox`), and the safety car in box 12. A car the game has not assigned (0x7fffffff) reads nothing.
- **Empty** when the live state is off (the experimental feature `telemetry`), on a build where its pit lane group is off, or on a game header that does not export `PitLaneVehicle.garage_index`. The fields are then 0 with `SWOFTY_LIVE_HAVE_GARAGE` clear.
- **The pit lane moments** are `session.pit` (a car's pit status changes) and `session.pit_stop` (a stop is counted) and `session.pit_stop.tyres` (the crew fitted the tyres) in [`swofty/systems/sessions.hpp`](/std/systems/sessions). A car's place now is `live::pit_status` and its stop count `live::pit_stops`.
- **Not yet:** the pit box pose and the distance to the box, which wait for the prototypes of `Game_PitLane_GetGaragePose_Thunk` and `Game_PitLane_GetDistanceToBox`.
- **Builds:** the garage offset is witnessed on the KB build and Steam 1.0.145 (`Game_PitLane_AssignGarageIndices`, 450 of 450 instructions equal). The read compiles in once the game header exports `PitLaneVehicle.garage_index`.
