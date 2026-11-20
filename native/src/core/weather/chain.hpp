#pragma once

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

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

// One dataset's build, the arrays Sess_Tod_BuildSessionDataset_Thunk takes with use_count set: state i holds for
// hold[i] seconds, then changes into state i + 1 over trans[i] seconds; the last state's transition is 0. An
// intensity below 0 leaves the state's intensity to the game's draw. The rules below are Career Mode Editor's
// (validate_build, unsupported_state, clamp_build), so a script the core builds is held to the same limits as a plan.
inline constexpr unsigned MAX_STATES = 6;
inline constexpr float MIN_HOLD_S = 1.0f, MIN_TRANSITION_S = 1.0f;
inline constexpr const char *KIND_NAMES[MAX_STATES] = {"clear", "light_cloud", "overcast", "light_rain", "heavy_rain", "storm"};

struct Build {
    uint32_t n = 0;
    uint32_t dropped = 0;
    uint8_t type[MAX_STATES] = {};
    float hold[MAX_STATES] = {};
    float trans[MAX_STATES] = {};
    float intensity[MAX_STATES] = {-1, -1, -1, -1, -1, -1};
};

// One scripted state: its weather type (0 clear .. 5 storm), how long it holds, how long the change into it takes,
// and its intensity (below 0: the game's draw). The first slot's transition is unused and the last slot holds to
// the end of the session.
struct Slot {
    uint8_t kind = 0;
    float duration_s = 0, transition_s = 0, intensity = -1;
};

// A TimeOfDaySystem weather type range (intensity_min, intensity_max).
struct TypeRange {
    float min = 0, max = 0;
};

inline bool usable(const TypeRange &r)
{
    return std::isfinite(r.min) && std::isfinite(r.max) && r.min >= 0 && r.min <= r.max && r.max <= 1;
}

// The dataset length the build uses: the game's when it is a plausible session (1 minute to 10 hours), else an hour,
// or two for the race datasets (5 R, 6 R2, 7 R3).
inline float session_length(float game_len_s, unsigned dataset)
{
    if (std::isfinite(game_len_s) && game_len_s >= 60.0f && game_len_s <= 36000.0f)
        return game_len_s;
    return dataset >= 5 ? 7200.0f : 3600.0f;
}

// The script as one build over len seconds. A slot that leaves no room for a transition and a hold before the
// session ends, and every slot past the sixth, is dropped and counted.
inline Build build_of(const Slot *slots, unsigned count, float len)
{
    Build b;
    if (!count)
        return b;
    b.type[0] = slots[0].kind;
    b.intensity[0] = slots[0].intensity;
    b.n = 1;
    float in_at = 0;
    unsigned k = 1;
    for (; k < count && b.n < MAX_STATES; ++k) {
        float hold = std::fmax(slots[k - 1].duration_s, MIN_HOLD_S);
        float ramp = std::fmax(slots[k].transition_s, MIN_TRANSITION_S);
        if (!std::isfinite(hold) || !std::isfinite(ramp) || in_at + hold + ramp + MIN_HOLD_S > len)
            break;
        b.hold[b.n - 1] = hold;
        b.trans[b.n - 1] = ramp;
        in_at += hold + ramp;
        b.type[b.n] = slots[k].kind;
        b.intensity[b.n] = slots[k].intensity;
        ++b.n;
    }
    b.dropped = count - k;
    b.hold[b.n - 1] = std::fmax(len - in_at, MIN_HOLD_S);
    b.trans[b.n - 1] = 0;
    return b;
}

// The plain build a failed rebuild falls back to: one state, the game's first type when it has one.
inline Build plain_build(int game_first, float len)
{
    Build b;
    b.n = 1;
    b.type[0] = uint8_t(game_first >= 0 && game_first < int(MAX_STATES) ? game_first : 0);
    b.hold[0] = len;
    return b;
}

// "" when every hold, transition and intensity is finite and in range and the states fit the session, else what is
// wrong. A build that fails is not handed to the game.
inline std::string validate(const Build &b, float len)
{
    char buf[160];
    if (!std::isfinite(len) || len <= 0)
        return "session length is not a positive number";
    if (b.n < 1 || b.n > MAX_STATES) {
        std::snprintf(buf, sizeof buf, "%u states (1..%u allowed)", b.n, MAX_STATES);
        return buf;
    }
    double total = 0;
    for (uint32_t i = 0; i < b.n; ++i) {
        bool last = i + 1 == b.n;
        if (b.type[i] >= MAX_STATES) {
            std::snprintf(buf, sizeof buf, "state %u has type %u", i, b.type[i]);
            return buf;
        }
        if (!std::isfinite(b.hold[i]) || b.hold[i] < MIN_HOLD_S) {
            std::snprintf(buf, sizeof buf, "state %u hold %g s is not a finite time of at least %g s", i, b.hold[i], MIN_HOLD_S);
            return buf;
        }
        if (!std::isfinite(b.trans[i]) || (last ? b.trans[i] != 0 : b.trans[i] < MIN_TRANSITION_S)) {
            std::snprintf(buf, sizeof buf, "state %u transition %g s is not finite or is shorter than %g s", i, b.trans[i],
                          MIN_TRANSITION_S);
            return buf;
        }
        if (!std::isfinite(b.intensity[i]) || b.intensity[i] > 1.0f) {
            std::snprintf(buf, sizeof buf, "state %u intensity %g is not a number up to 1", i, b.intensity[i]);
            return buf;
        }
        total += double(b.hold[i]) + b.trans[i];
    }
    if (!(total <= double(len) * 1.0001 + 0.01)) {
        std::snprintf(buf, sizeof buf, "states add up to %g s, past the %g s session", total, len);
        return buf;
    }
    return "";
}

// The first wet state (light_rain, heavy_rain, storm) the track's type range cannot draw, or "". A script with one
// keeps the game's build.
inline std::string unsupported_state(const Build &b, const TypeRange (&ranges)[MAX_STATES])
{
    for (uint32_t i = 0; i < b.n; ++i) {
        uint8_t t = b.type[i];
        if (t < 3 || t >= MAX_STATES)
            continue;
        if (!usable(ranges[t]) || ranges[t].max <= 0.0f)
            return KIND_NAMES[t];
    }
    return "";
}

// Moves each set intensity into its type's range, or back to the game's draw when that range is not a usable 0..1
// interval. One line per changed state.
inline std::vector<std::string> clamp(Build &b, const TypeRange (&ranges)[MAX_STATES])
{
    std::vector<std::string> out;
    for (uint32_t i = 0; i < b.n; ++i) {
        float v = b.intensity[i];
        if (v < 0 || b.type[i] >= MAX_STATES)
            continue;
        const TypeRange &r = ranges[b.type[i]];
        bool ok = usable(r);
        float to = !ok ? -1.0f : v < r.min ? r.min : v > r.max ? r.max : v;
        if (to == v)
            continue;
        b.intensity[i] = to;
        char buf[128];
        if (ok)
            std::snprintf(buf, sizeof buf, "state %u %s intensity %.2f -> %.2f (game range %.2f..%.2f)", i, KIND_NAMES[b.type[i]],
                          v, to, r.min, r.max);
        else
            std::snprintf(buf, sizeof buf, "state %u %s intensity %.2f -> the game's draw (game range %g..%g unusable)", i,
                          KIND_NAMES[b.type[i]], v, r.min, r.max);
        out.push_back(buf);
    }
    return out;
}

// One state of a dataset the game built: when it ends, its transition and its intensity.
struct BuiltState {
    uint8_t type = 0;
    float end = 0, transition = 0, intensity = 0;
};

// True when every built state is finite and, with in_range, not negative and its intensity at most 1.
inline bool dataset_finite(const BuiltState *states, uint32_t n, bool in_range = true)
{
    for (uint32_t i = 0; i < n; ++i) {
        const BuiltState &st = states[i];
        if (!std::isfinite(st.end) || !std::isfinite(st.transition) || !std::isfinite(st.intensity))
            return false;
        if (in_range && (st.end < 0 || st.transition < 0 || st.intensity < 0 || st.intensity > 1))
            return false;
    }
    return true;
}

// True when the game's dataset holds the build's states in order.
inline bool holds(const BuiltState *states, uint32_t n, const Build &b)
{
    if (n < b.n)
        return false;
    for (uint32_t i = 0; i < b.n; ++i)
        if (states[i].type != b.type[i])
            return false;
    return true;
}

inline constexpr const char *DATASET_KEYS[DATASETS] = {"p1", "p2", "p3", "q", "ss", "r", "r2", "r3"};

// SWOFTY_WEATHER_TEST: a seed and scripts the core sets on session.weather.setup before the handlers run, for
// checking the chain in game without an addon. "seed=1990;r=clear:1200,overcast:600:300,heavy_rain:0:120:0.8": a
// seed, then per dataset key (DATASET_KEYS) up to six slots of kind:duration_s[:transition_s[:intensity]].
struct TestScript {
    bool has_seed = false;
    uint32_t seed = 0;
    uint8_t mask = 0;
    uint8_t counts[DATASETS] = {};
    Slot slots[DATASETS][MAX_STATES] = {};
};

inline int kind_of(const std::string &s)
{
    for (unsigned k = 0; k < MAX_STATES; k++)
        if (s == KIND_NAMES[k])
            return int(k);
    return -1;
}

inline std::vector<std::string> split(const std::string &s, char sep)
{
    std::vector<std::string> out;
    size_t at = 0;
    for (;;) {
        size_t next = s.find(sep, at);
        out.push_back(s.substr(at, next == std::string::npos ? std::string::npos : next - at));
        if (next == std::string::npos)
            return out;
        at = next + 1;
    }
}

inline bool number(const std::string &s, float &out)
{
    if (s.empty())
        return false;
    char *end = nullptr;
    double v = std::strtod(s.c_str(), &end);
    if (end != s.c_str() + s.size() || !std::isfinite(v))
        return false;
    out = float(v);
    return true;
}

// "" when text parses into t, else what is wrong with it.
inline std::string parse_test_script(const std::string &text, TestScript &t)
{
    t = TestScript{};
    for (const std::string &part : split(text, ';')) {
        if (part.empty())
            continue;
        size_t eq = part.find('=');
        if (eq == std::string::npos)
            return "\"" + part + "\" is not key=value";
        std::string key = part.substr(0, eq), value = part.substr(eq + 1);
        if (key == "seed") {
            char *end = nullptr;
            unsigned long v = std::strtoul(value.c_str(), &end, 10);
            if (value.empty() || end != value.c_str() + value.size() || v == 0 || v > 0xffffffffull)
                return "seed \"" + value + "\" is not a number 1..4294967295";
            t.has_seed = true;
            t.seed = uint32_t(v);
            continue;
        }
        unsigned d = 0;
        while (d < DATASETS && key != DATASET_KEYS[d])
            d++;
        if (d == DATASETS)
            return "\"" + key + "\" is not seed or a dataset key (p1 p2 p3 q ss r r2 r3)";
        std::vector<std::string> slots = split(value, ',');
        if (slots.size() > MAX_STATES)
            return key + " has " + std::to_string(slots.size()) + " slots (up to " + std::to_string(MAX_STATES) + ")";
        for (unsigned i = 0; i < slots.size(); i++) {
            std::vector<std::string> f = split(slots[i], ':');
            int kind = kind_of(f[0]);
            if (kind < 0)
                return key + " slot " + std::to_string(i) + ": \"" + f[0] + "\" is not a weather kind";
            if (f.size() < 2 || f.size() > 4)
                return key + " slot " + std::to_string(i) + ": want kind:duration_s[:transition_s[:intensity]]";
            Slot &s = t.slots[d][i];
            s.kind = uint8_t(kind);
            if (!number(f[1], s.duration_s) || (f.size() > 2 && !number(f[2], s.transition_s)) ||
                (f.size() > 3 && !number(f[3], s.intensity)))
                return key + " slot " + std::to_string(i) + ": \"" + slots[i] + "\" has a field that is not a number";
        }
        t.counts[d] = uint8_t(slots.size());
        t.mask = uint8_t(t.mask | (1u << d));
    }
    return "";
}

}
