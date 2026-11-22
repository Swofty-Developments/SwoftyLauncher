---
title: Pit stops
description: Time penalties, extra box holds and the addon rows on the MFD pit page, from swofty/systems/sessions.hpp and swofty/systems/pit_rows.hpp.
---

# Pit stops

The pit stop calls read and change a car's time penalty, hold a car in its pit box for extra seconds at its next stop, and drive the NO / YES rows an addon adds to the race MFD's pit page. Together they let an addon serve a time penalty in the pits.

```cpp [native/src/serve_penalty.cpp]
#include <swofty/events.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/pit_rows.hpp>
#include <swofty/systems/sessions.hpp>

using namespace swofty;

std::optional<hud::PitRow> g_row;
uint32_t g_serves = 0;

void frame(double, void *)
{
    if (!g_row && !(g_row = hud::PitRow::find("serve_penalty")))
        return;
    cars::live::Car me(0);
    double penalty = sessions::time_penalty(me).value_or(0.0);
    g_row->lock(penalty < 0.05);
    sessions::set_pit_hold(me, g_row->yes() ? float(penalty) : 0.0f);
    auto hold = sessions::pit_hold(me);
    if (hold && hold->serves > g_serves) {
        g_serves = hold->serves;
        sessions::set_time_penalty(me, std::max(0.0, penalty - hold->served));
        g_row->set(false);
    }
}

SWOFTY_REGISTER
{
    events::on_frame(frame);
}
```

The manifest declares the row:

```json
"hud": {
  "pit_rows": [{ "id": "serve_penalty", "label": "Serve Penalty" }]
}
```

## Time penalties

```cpp
namespace swofty::sessions {

std::optional<double> time_penalty(cars::live::Car car);   // seconds added to the car's race time
Result set_time_penalty(cars::live::Car car, double seconds);   // 0 to 3600

}
```

The penalty is the one the classification adds at the end of the race and the HUD shows next to the car. `time_penalty` is empty before the session's rules start and for a car the session has no result for. `set_time_penalty` answers `late` then, and `invalid` for a negative, infinite or over-3600 value. A changed penalty shows on the HUD and the results screen as if the stewards had set it. The core logs every write as `[sessions] time penalty of car <n>: <new> s, was <old> s`.

## Box holds

```cpp
namespace swofty::sessions {

enum class PitCrew { none, stretch, freeze };

struct PitHold {
    float armed;      // extra seconds the car's next stop holds
    float holding;    // extra seconds of the stop in progress, 0 outside the box
    float served;     // extra seconds the last served stop held
    uint32_t serves;  // stops that served extra seconds this session
    PitCrew crew;     // how the crew waited at the last served stop
};

Result set_pit_hold(cars::live::Car car, float seconds);   // 0 disarms; at most 600
std::optional<PitHold> pit_hold(cars::live::Car car);

}
```

`set_pit_hold` arms the car's next stop: the car stays in its box for that many seconds more than the crew's own work. Any length works, 3 s, 15 s or 20 s. The armed seconds are fixed when the car enters the pit lane, so a change after that applies to the stop after. A stop serves its seconds when the car leaves the box; `serves` goes up by one, `served` holds the seconds and `armed` goes back to 0. A new session starts every car over.

The crew waits through the extra seconds before it works, the way it does for a stop-go penalty:

- **stretch** (the default): the crew plays the game's stop-go wait, timed to the hold, then works at normal speed.
- **freeze**: the crew stands still for the hold, then does a normal stop. A stop with no stop-go animation falls back to it.
- **none**: the crew works at once and the car waits afterwards.

The core config's `pit_hold_crew` (`stretch`, `freeze` or `none`) or `SWOFTY_PIT_HOLD_CREW` picks the style. `SWOFTY_PIT_HOLD=0` turns holds off; `set_pit_hold` then answers `unavailable`.

## Pit page rows

```cpp
namespace swofty::hud {

class PitRow {
public:
    static std::optional<PitRow> find(const char *id);   // the row by its hud.pit_rows id
    bool yes() const;              // the player's pick
    bool set(bool yes) const;      // changes the pick
    bool lock(bool locked) const;  // greys the row out and holds it on NO
};

}
```

Each row of `hud.pit_rows` sits under Front Wing Repair on the MFD pit page and shows NO or YES, starting on NO. `find` names only the calling addon's rows and is empty until the page's rows are built in game, or when the row was left out (more than 2 addon rows in the session). A locked row is greyed out the way the game greys its own rows in the pit lane, and reads NO. The player changes an unlocked row with the MFD's left and right inputs. `SWOFTY_PIT_ROWS=0` leaves the rows out.

## Builds

Steam 1.0.141 and 1.0.145. On a build where a call's hooks don't install, `set_pit_hold`, `pit_hold` and `PitRow::find` answer `unavailable` or empty and the core log names the missing binding under `[pit hold]` or `[pit rows]`.
