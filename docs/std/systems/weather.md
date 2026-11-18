---
title: Weather
description: Track climate and scripted forecasts in code, with the weather setup event and the seed and script operations.
---

# Weather

With the weather system you change how often it rains at a track, and script a session's weather slot by slot. A wet-race addon, for example, makes the Belgian Grand Prix start dry and end in heavy rain.

A **climate** is a track's weather weights and temperature range. A **forecast** is a script for one session: a list of **slots**, each a kind of weather, how long it lasts and how long the change into it takes.

::: info In 1.10.0
`swofty/systems/weather.hpp` ships the `session.weather.setup` event and its payload helpers, [below](#the-setup-event). The event fires only with `SWOFTY_WEATHER_CHAIN=1` on the KB build and Steam 1.0.145. The registries in [The header](#the-header) are the design.
:::

## The setup event

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
        weather::set_seed(e, 1990);
        std::array<weather::Slot, 3> race{{
            {Kind::clear, {}, 20 * 60.0f, 0, -1},          // dry for 20 minutes
            {Kind::overcast, {}, 10 * 60.0f, 5 * 60.0f, -1},  // clouding over for 5 minutes, overcast for 10
            {Kind::heavy_rain, {}, 0, 2 * 60.0f, 0.8f},     // rain to the flag
        }};
        weather::script(e, weather::Dataset::race, race);
    });
}
```

```cpp
namespace swofty::weather {

enum class Kind : uint8_t { clear, light_cloud, overcast, light_rain, heavy_rain, storm };
enum class Dataset : uint8_t { practice_1, practice_2, practice_3, qualifying, sprint_shootout, race, race_2, race_3 };

struct Slot {
    Kind kind;
    uint8_t reserved[3];
    float duration_s;    // how long it holds; the last slot holds to the end of the session
    float transition_s;  // how long the change into it takes; unused on the first slot
    float intensity;     // 0..1, or below 0 for the game's draw
};

namespace events {
struct Setup {                                 // Sess_GameWeatherSystem_SetupSession (modify)
    static constexpr const char *point = "session.weather.setup";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t session_type;                     // the game's session type
    uint8_t game_scripted;                     // the game scripts this setup itself: no script applies
    uint8_t time_trial;                        // a Time Trial setup: no script applies
    uint8_t seed_forced;                       // a seed is forced above the core: set_seed is dropped
    uint8_t set_seed;
    uint32_t seed;
    uint8_t script_mask;                       // bit d: dataset d is scripted
    uint8_t counts[8];
    uint8_t reserved[3];
    Slot slots[8][6];
};
}

void set_seed(events::Setup &e, uint32_t seed);
void script(events::Setup &e, Dataset d, std::span<const Slot> slots);  // up to 6 slots

}
```

- **Order:** before handlers run, then the game builds every dataset with the handler's seed, then the core rebuilds each scripted dataset through `Sess_Tod_BuildSessionDataset_Thunk`, then Career Mode Editor rebuilds the datasets it plans. The seed is released after the build.
- **The seed:** a seed already forced above the core (Career Mode Editor's pin) is kept, `seed_forced` reads 1, and `set_seed` is dropped with a line.
- **A script** is held to Career Mode Editor's build rules. A slot with no room for a 1 s transition and a 1 s hold before the session ends is dropped. An intensity moves into the track's range for its kind. A wet kind the track cannot draw keeps the game's forecast. A dataset the game builds non-finite is rebuilt as one plain state.
- **Without an addon:** `SWOFTY_WEATHER_TEST` sets a seed and scripts on every setup before the handlers run, as `seed=1990;r=clear:1200,overcast:600:300,heavy_rain:0:120:0.8`: per dataset key (`p1 p2 p3 q ss r r2 r3`) up to six slots of `kind:duration_s[:transition_s[:intensity]]`.
- **Log lines:** `[weather] setup chain armed (SWOFTY_WEATHER_CHAIN=1): …` at boot, then per setup `[weather] session type <n>: seed <s> (a handler) forced for this build, released after it`, and per scripted dataset `[weather] r: rebuilt from the script (3 state(s))` or `[weather] r: not applied: <why>`.

## The header

<Split>

`swofty/systems/weather.hpp` has two registries:

- `weather::climates`: a track's weather weights and temperatures (`weather.track`);
- `weather::forecasts`: a scripted forecast for one session (`weather.forecast`).

The core applies a forecast at `session.weather.setup`, the moment Career Mode Editor's weather module also uses.

Career Mode Editor's weather plan wins over an addon's forecast. In a career where Career Mode Editor plans a session's weather or pins its seed, its datasets and seed replace the forecast and seed `session.weather.setup` handlers set. An addon's forecast applies to the sessions and datasets Career Mode Editor has no plan for.

<template #example>
<Example root="swofty-sdk" code-file="include/swofty/systems/weather.hpp">
<template #code>

```cpp
#pragma once
#include <swofty/registry.hpp>
#include <swofty/events.hpp>
#include <swofty/systems/sessions.hpp>
#include <swofty/systems/tracks.hpp>

namespace swofty::weather {

// Kind, Slot and Dataset are the shipped ones (see The setup event).

struct Climate {
    Ref<tracks::Track> track;                  // tracks.Track_N.Weather_N
    std::array<float, 5> weights;              // clear, cloudy, overcast, light rain, heavy rain
    float temperature_min;                     // degrees C
    float temperature_max;
};
inline Registry<Climate> climates{"weather.track", { .applies = Applies::boot }};

struct Forecast {
    events::Selector within;                   // where it applies: a content, a data group, a track
    sessions::Session session;                 // which session of the weekend
    std::vector<Slot> slots;                   // the script, in order
    std::optional<uint32_t> seed;              // a fixed random seed for what the script leaves open
};
inline Registry<Forecast> forecasts{"weather.forecast", { .applies = Applies::session, .snapshot = true }};

// session.weather.setup is weather::events::Setup, shipped in 1.10.0: see The setup event.

Result set_seed(sessions::Session session, uint32_t seed);              // before the session's setup
Result script(sessions::Session session, std::span<const Slot> slots);

} // namespace swofty::weather
```

</template>
</Example>
</template>
</Split>

## A scripted forecast

<Split>

A wet-race addon scripts the Belgian Grand Prix: dry for 20 minutes, clouding over, then rain to the flag. It applies in every career and every Grand Prix box that races at Spa, and only while its `storm` option is on.

Qualifying keeps the game's weather, with a fixed seed so every player gets the same one.

<template #example>
<Example root="mod.spa-storm" code-file="native/src/forecast.cpp" data-file="registry/weather.json" files="swofty.json, native/CMakeLists.txt">
<template #code>

```cpp
#include <swofty/registry.hpp>
#include <swofty/events.hpp>
#include <swofty/systems/weather.hpp>

using namespace swofty;
using weather::Kind;

static const events::Selector kSpa = events::track("game/spa");

SWOFTY_REGISTER
{
    auto race = weather::forecasts.add("spa_race", {
        .within = kSpa,
        .session = sessions::Session::race,
        .slots = {
            { Kind::clear,      {}, 20 * 60.0f, 0,        -1 },
            { Kind::overcast,   {}, 15 * 60.0f, 5 * 60.0f, -1 },
            { Kind::light_rain, {}, 20 * 60.0f, 5 * 60.0f, -1 },
            { Kind::heavy_rain, {}, 60 * 60.0f, 3 * 60.0f, -1 },
        },
    });

    weather::forecasts.layer(race, [](weather::Forecast &f) {
        if (!option<bool>("storm"))
            f.slots.clear();                               // no slots: the game's own weather
    });
}

SWOFTY_ON_LOAD
{
    events::on<weather::events::Setup>().in(kSpa).when(events::session_type({ events::SessionType::qualifying }))
        .before([](weather::events::Setup &w, const events::Context &) {
            weather::set_seed(w, 1990);
        });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
{
  "weather.forecast": {
    "spa_race": {
      "within": { "track": "game/spa" },
      "session": "race",
      "slots": [
        { "kind": "clear",      "duration": 20, "transition": 0 },
        { "kind": "overcast",   "duration": 15, "transition": 5 },
        { "kind": "light_rain", "duration": 20, "transition": 5 },
        { "kind": "heavy_rain", "duration": 60, "transition": 3 }
      ]
    }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] register: mod.spa-storm (spa_storm.dll) 1 weather.forecast, 1 layer on weather.forecast (early stage, 0.1 ms)
[events] session.weather.setup (driver_career, swofty.progressive/f1_career_progressive, race, game/spa):
         forecast mod.spa-storm/spa_race applied by the core (4 slots); 0 before handlers
[events] session.weather.setup (grand_prix, qualifying_1, game/spa): 1 before handler (mod.spa-storm): seed 1990
```

The data version is always on. It can't read the `storm` option, and it has no seed handler.

A career freezes the forecasts that apply to a season when that season starts.

## Climate

A climate change reaches every box and career that races at the track, except a box that sets its own climate there.

```cpp
// a wetter Silverstone, everywhere
weather::climates.layer(weather::climates["game/silverstone"], [](weather::Climate &c) {
    c.weights = { 0.30f, 0.25f, 0.20f, 0.15f, 0.10f };
    c.temperature_max = 21.0f;
});
```

## Events and operations

```cpp
// rain at the end of every race in 1990 careers
events::on<weather::events::Setup>().in(events::content("swofty.f1-1990/f1_career_1990"))
    .when(events::session_type({ events::SessionType::race }))
    .before([](weather::events::Setup &w, const events::Context &) { weather::script(w, weather::Dataset::race, late_rain); });

// a launcher page asked for a fixed seed: Career Mode Editor's "weather seed"
weather::set_seed(sessions::Session::race, 424242);
weather::script(sessions::Session::practice_1, std::array{ weather::Slot{ Kind::heavy_rain, {}, 3600, 0, -1 } });
```

## Readiness and limits

- **Ready** for setup and seeds: `Sess_GameWeatherSystem_SetupSession`, the seed functions and the dataset build are L2, shipped in Career Mode Editor.
- **Partial:** the dataset layout is L1.
- **Unknown:** the track wetness and drying model, and how rain intensity changes grip.
- **The front-end scene loader** rebuilds dataset 0 as clear (0x1428c1ce0).
- **A forced seed persists in saves.**
- **Career Mode Editor** wraps `Sess_GameWeatherSystem_SetupSession` at priority 0 and the core at -100. A setup runs Career Mode Editor's seed push, the core's handlers, the game's build, the core's rebuild of the datasets a handler scripts, then Career Mode Editor's rebuild of the datasets it plans.
- **The seed:** the game zeroes `GameWeatherSystem.forced_seed` (ws+0x1f14) and never sets it. A non-zero value when the core's wrap starts is a seed forced above the core, such as Career Mode Editor's pin: the core keeps it and drops a handler's seed with a line. With nothing forced, a handler's seed is forced for that build and released after it.
- **Rain quality settings** (`rainBeads`, `rainSplashes`, `rainHigh`, `trackHeatHaze`) are graphics settings, see [Graphics](./graphics).
