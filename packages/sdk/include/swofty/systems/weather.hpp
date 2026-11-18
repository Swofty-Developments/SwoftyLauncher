// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#include <span>
#include <swofty/events.hpp>

// Weather: session.weather.setup, fired when the game sets up a session's weather (Sess_GameWeatherSystem_SetupSession).
// A before handler sets the session's random seed with set_seed and scripts datasets with script; the core forces the
// seed for that build and rebuilds each scripted dataset after the game's own build. Career Mode Editor's weather plan
// wins: a seed it forced is kept (seed_forced is set and set_seed is dropped), and a dataset it plans is rebuilt from
// its plan after the core's script. A Time Trial setup and the game's own scripted weather (game_scripted) take no
// script. The event needs SWOFTY_WEATHER_CHAIN=1 on a build where the weather setup layout is proven (the KB build and
// Steam 1.0.145); elsewhere it is not fired.

namespace swofty::weather {

enum class Kind : uint8_t { clear = 0, light_cloud = 1, overcast = 2, light_rain = 3, heavy_rain = 4, storm = 5 };

// The game's weather datasets, one per session of the weekend.
enum class Dataset : uint8_t {
    practice_1 = 0,
    practice_2 = 1,
    practice_3 = 2,
    qualifying = 3,
    sprint_shootout = 4,
    race = 5,
    race_2 = 6,
    race_3 = 7,
};

inline constexpr unsigned DATASETS = 8;
inline constexpr unsigned MAX_SLOTS = 6;

// One scripted state. duration_s: how long it holds (the last slot holds to the end of the session). transition_s:
// how long the change into it takes (unused on the first slot). intensity: 0..1 inside the track's range for the
// kind, or below 0 for the game's draw.
struct Slot {
    Kind kind;
    uint8_t reserved[3];
    float duration_s;
    float transition_s;
    float intensity;
};

namespace events {

struct Setup {                                 // Sess_GameWeatherSystem_SetupSession (modify)
    static constexpr const char *point = "session.weather.setup";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t session_type;                     // the game's session type of the session being set up
    uint8_t game_scripted;                     // the game scripts this setup itself: no dataset takes a script
    uint8_t time_trial;                        // a Time Trial setup: no dataset takes a script
    uint8_t seed_forced;                       // a seed is already forced above the core: set_seed is dropped
    uint8_t set_seed;                          // a before handler set seed
    uint32_t seed;                             // the seed the build uses; a before handler sets it with set_seed
    uint8_t script_mask;                       // bit d: dataset d is scripted
    uint8_t counts[DATASETS];                  // slots[d][0 .. counts[d]) are dataset d's script
    uint8_t reserved[3];
    Slot slots[DATASETS][MAX_SLOTS];
};

} // namespace events

inline void set_seed(events::Setup &e, uint32_t seed)
{
    e.seed = seed;
    e.set_seed = 1;
}

// Scripts one dataset; past MAX_SLOTS the rest are left out. An empty script clears the dataset's script.
inline void script(events::Setup &e, Dataset d, std::span<const Slot> slots)
{
    unsigned i = unsigned(d);
    if (i >= DATASETS)
        return;
    unsigned n = slots.size() < MAX_SLOTS ? unsigned(slots.size()) : MAX_SLOTS;
    for (unsigned k = 0; k < n; k++)
        e.slots[i][k] = slots[k];
    e.counts[i] = uint8_t(n);
    if (n)
        e.script_mask = uint8_t(e.script_mask | (1u << i));
    else
        e.script_mask = uint8_t(e.script_mask & ~(1u << i));
}

inline const char *name(Kind k)
{
    static const char *const names[] = {"clear", "light_cloud", "overcast", "light_rain", "heavy_rain", "storm"};
    return unsigned(k) < 6 ? names[unsigned(k)] : "unknown";
}

} // namespace swofty::weather
