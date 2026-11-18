#pragma once

#include <cstdint>
#include <string>

// The order rules of the weather setup chain (docs/std/systems/weather.md). Career Mode Editor wraps
// Sess_GameWeatherSystem_SetupSession at priority 0 and the core at PRIORITY, below it, so a setup runs Career Mode
// Editor, then the core's session.weather.setup handlers, then the game's build, then the core's rebuild of the
// datasets an addon scripted, then Career Mode Editor's rebuild of the datasets it planned. Career Mode Editor's plan
// wins over an addon's forecast, and the forecast applies where Career Mode Editor has no plan.
//
// The seed: GameWeatherSystem.forced_seed (ws+0x1f14) is 0 unless an addon wrote it; the constructor zeroes it and
// never sets it (re/bindings/session/Sess_GameWeatherSystem_UpdateRandomSeed.json). A non-zero value at the core's
// entry is a seed forced above the core, and the core keeps it. With nothing forced, a handler's seed is forced for
// this build and released after it (set back to 0 and propagated with Sess_WeatherComponent_SetSystemRandomSeed, as
// Career Mode Editor releases its pin), so it does not carry into the next session or a save.
//
// Pure: the wrap reads the weather system, asks these functions, and writes what they return.

namespace swofty_core::weather {

inline constexpr int PRIORITY = -100;
inline constexpr int CAREER_MODE_EDITOR_PRIORITY = 0;
inline constexpr unsigned DATASETS = 8;

enum class SeedAction : uint8_t {
    none,
    keep_forced,
    force,
};

struct Seed {
    SeedAction action = SeedAction::none;
    uint32_t value = 0;
    bool release_after = false;
    bool dropped = false;
};

// forced_at_entry: ws+0x1f14 when the core's wrap starts. requested: the seed a handler set, when one did.
inline Seed seed_rule(uint32_t forced_at_entry, bool requested, uint32_t seed)
{
    Seed s;
    if (forced_at_entry != 0) {
        s.action = SeedAction::keep_forced;
        s.value = forced_at_entry;
        s.dropped = requested && seed != forced_at_entry;
        return s;
    }
    if (!requested || seed == 0)
        return s;
    s.action = SeedAction::force;
    s.value = seed;
    s.release_after = true;
    return s;
}

enum class Owner : uint8_t { game, addon, career_mode_editor };

inline const char *owner_name(Owner o)
{
    return o == Owner::career_mode_editor ? "Career Mode Editor" : o == Owner::addon ? "addon" : "game";
}

// The datasets the core rebuilds after the game's build: those a handler scripted. A scripted setup (the game's own
// scripted weather, settings exact_timing set) and a Time Trial setup are the game's alone.
inline uint8_t core_rebuilds(uint8_t scripted_mask, bool game_scripted, bool time_trial)
{
    return game_scripted || time_trial ? 0 : scripted_mask;
}

// Who built each dataset once the whole chain has returned: Career Mode Editor's rebuild runs last, over the core's.
inline Owner owner_of(unsigned dataset, uint8_t core_rebuilt, uint8_t career_mode_editor_rebuilt)
{
    uint8_t bit = uint8_t(1u << dataset);
    if (career_mode_editor_rebuilt & bit)
        return Owner::career_mode_editor;
    if (core_rebuilt & bit)
        return Owner::addon;
    return Owner::game;
}

inline std::string seed_line(const Seed &s, const std::string &addon)
{
    switch (s.action) {
    case SeedAction::keep_forced:
        return "seed " + std::to_string(s.value) + " forced above the core kept" +
               (s.dropped ? "; " + addon + "'s seed not applied" : std::string());
    case SeedAction::force:
        return "seed " + std::to_string(s.value) + " (" + addon + ") forced for this build, released after it";
    case SeedAction::none:
        break;
    }
    return "the game's seed";
}

// The TimeOfDaySystem flags Sess_Tod_BuildSessionDataset_Thunk's body tests before it builds (witnessed on KB and
// Steam 1.0.145 in its record): busy +0xb432, ready +0xb429 and +0xb42a, override +0xb436 with random +0xb42d or
// scripted +0x2654.
struct TodFlags {
    bool busy = false, ready_a = false, ready_b = false;
    bool override_active = false, random_override = false, scripted_scenario = false;
};

// Why a rebuild of one dataset would not build what the core asks for, or "" when it would. The same gates, in the
// same order, as Career Mode Editor's rebuild.
inline const char *rebuild_gate(const TodFlags &t)
{
    if (t.busy)
        return "TimeOfDaySystem busy (TOD+0xb432 set): the builder would do nothing";
    if (!t.ready_a || !t.ready_b)
        return "TimeOfDaySystem not ready (TOD+0xb429/+0xb42a clear): the builder would do nothing";
    if (t.override_active && t.random_override)
        return "TimeOfDaySystem random override (TOD+0xb436 and +0xb42d set): the build makes random states instead";
    if (t.override_active && t.scripted_scenario)
        return "TimeOfDaySystem scripted override (TOD+0xb436 and +0x2654 set): the build runs the scripted scenario "
               "instead";
    return "";
}

}
