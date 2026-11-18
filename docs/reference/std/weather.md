---
title: Weather setup
description: The session.weather.setup event and its seed and script helpers from swofty/systems/weather.hpp.
---

# Weather setup

The weather setup library gives a `before` handler on `session.weather.setup` the session's random seed and a script of up to six weather states for each of the weekend's eight datasets. The core forces the seed for the game's build and rebuilds each scripted dataset after it. A wet-race addon starts a race dry and ends it in rain; a league addon gives every player the same practice weather from a fixed seed.

```cpp [native/src/forecast.cpp]
#include <array>
#include <swofty/events.hpp>
#include <swofty/systems/weather.hpp>

namespace ev = swofty::events;
namespace weather = swofty::weather;
using weather::Kind;

SWOFTY_ON_LOAD
{
    ev::on<weather::events::Setup>().before([](weather::events::Setup &e, const ev::Context &) {
        if (e.time_trial || e.game_scripted)
            return;                                             // the core applies no script to these setups
        if (!e.seed_forced)
            weather::set_seed(e, 1990);                         // the same unscripted weather for every player
        std::array<weather::Slot, 3> race{{
            {Kind::clear, {}, 20 * 60.0f, 0, -1},               // dry for 20 minutes, the game's intensity
            {Kind::overcast, {}, 10 * 60.0f, 5 * 60.0f, -1},    // 5 minutes clouding over, then 10 overcast
            {Kind::heavy_rain, {}, 0, 2 * 60.0f, 0.8f},         // 2 minutes into heavy rain, to the flag
        }};
        weather::script(e, weather::Dataset::race, race);
    });
}
```

```cpp
namespace swofty::weather {

enum class Kind : uint8_t { clear, light_cloud, overcast, light_rain, heavy_rain, storm };
enum class Dataset : uint8_t { practice_1, practice_2, practice_3, qualifying, sprint_shootout, race, race_2, race_3 };

inline constexpr unsigned DATASETS = 8;
inline constexpr unsigned MAX_SLOTS = 6;

struct Slot {
    Kind kind;
    uint8_t reserved[3];
    float duration_s;      // how long it holds; the last slot holds to the end of the session
    float transition_s;    // how long the change into it takes; unused on the first slot
    float intensity;       // 0..1, or below 0 for the game's draw
};

namespace events {
struct Setup {                                 // Sess_GameWeatherSystem_SetupSession (modify)
    static constexpr const char *point = "session.weather.setup";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t session_type;                     // the game's session type of the session being set up
    uint8_t game_scripted;                     // the game scripts this setup itself: no script applies
    uint8_t time_trial;                        // a Time Trial setup (session type 0x12): no script applies
    uint8_t seed_forced;                       // a seed is already forced above the core: set_seed is dropped
    uint8_t set_seed;                          // a before handler set seed
    uint32_t seed;                             // the seed the build uses
    uint8_t script_mask;                       // bit d: dataset d is scripted
    uint8_t counts[DATASETS];                  // slots[d][0 .. counts[d]) are dataset d's script
    uint8_t reserved[3];
    Slot slots[DATASETS][MAX_SLOTS];
};                                             // 800 bytes
}

void set_seed(events::Setup &e, uint32_t seed);
void script(events::Setup &e, Dataset d, std::span<const Slot> slots);  // the first 6; empty clears the script
const char *name(Kind k);                                              // "clear" .. "storm"

}
```

- **Order of one setup:** Career Mode Editor's wrap (priority 0) pushes its seed, then the core's wrap (priority -100) fires the event and runs the `before` handlers, forces the handlers' seed, and calls the game's build. After the build the core rebuilds each scripted dataset through `Sess_Tod_BuildSessionDataset_Thunk` and releases its seed. Career Mode Editor then rebuilds the datasets it plans, so its plan wins over a script.
- **The seed:** `seed` reads the forced seed when one is set above the core, else the game's base seed. A seed already forced (Career Mode Editor's pin) is kept: `seed_forced` reads 1 and `set_seed` is dropped. A seed of 0 forces nothing. The core forces the seed with `ws+0x1f14` and `Sess_GameWeatherSystem_UpdateRandomSeed`, and sets it back to 0 after the build.
- **A script** is built over the dataset's length: the game's when it is 1 minute to 10 hours, else an hour, or two for the race datasets. A slot with no room for a 1 s transition and a 1 s hold before the session ends is dropped with a line. Each intensity is clamped into the track's range for its kind (`TimeOfDaySystem.type_ranges`), or left to the game's draw when that range is not a usable 0..1 interval.
- **A script is refused**, and the game's forecast kept, when a wet kind (light rain, heavy rain, storm) has no range on the track, when the build does not fit the session, or when the time-of-day system is busy, not ready, or in a random or scripted override. A dataset the game builds non-finite is rebuilt as one plain state.
- **Not applied** to a Time Trial setup or to a setup the game scripts itself (`game_scripted`): the event fires and the scripts are dropped with a line.
- **Off** unless `SWOFTY_WEATHER_CHAIN=1`, and only on a build whose weather setup layout is proven (the KB build and Steam 1.0.145). Elsewhere the event is not fired.
- **Without an addon:** `SWOFTY_WEATHER_TEST=seed=1990;r=clear:1200,overcast:600:300,heavy_rain:0:120:0.8` sets a seed and scripts before the handlers run: per dataset key (`p1 p2 p3 q ss r r2 r3`) up to six slots of `kind:duration_s[:transition_s[:intensity]]`.

```text
[weather] setup chain armed (SWOFTY_WEATHER_CHAIN=1): session.weather.setup fires, below Career Mode Editor's weather wrap
[weather] session type <n>: seed 1990 (a handler) forced for this build, released after it
[weather] r: state 2 heavy_rain intensity 0.80 -> 0.70 (game range 0.40..0.70)
[weather] r: rebuilt from the script (3 state(s))
[weather] session type 18: a Time Trial setup; the scripts are not applied
[weather] r: not applied: refused: this track has no storm; the game's forecast is kept
```

- **Builds:** `Sess_GameWeatherSystem_SetupSession`, `Sess_Tod_BuildSessionDataset_Thunk` and `Sess_GameWeatherSystem_UpdateRandomSeed` are witnessed on the KB build and Steam 1.0.145. The weather system page is [Weather](/std/systems/weather).
