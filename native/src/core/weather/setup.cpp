#include "core/hooks/core_events.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/build_proof.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/introspection.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/stage.hpp"
#include "core/weather/chain.hpp"

#include <windows.h>

#include <swofty/game/session.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/systems/weather.hpp>

#include <cstdint>
#include <cstring>
#include <span>
#include <string>
#include <vector>

// The weather setup chain (core/weather/chain.hpp, docs/std/systems/weather.md): a wrap of
// Sess_GameWeatherSystem_SetupSession at priority -100, below Career Mode Editor's 0. On the way in it fires
// session.weather.setup and forces a handler's seed unless one is already forced above the core; it calls the game's
// build; on the way out it rebuilds each dataset a handler scripted through Sess_Tod_BuildSessionDataset_Thunk, held
// to Career Mode Editor's build rules, and releases the seed it forced. Career Mode Editor's own rebuild runs after
// this wrap returns, so its plan wins over a script. Off unless SWOFTY_WEATHER_CHAIN=1, and only on a build where the
// weather setup layout is proven.

namespace {

namespace wx = swofty_core::weather;
namespace w = swofty::weather;
namespace g = swofty::game;
namespace bp = runtime::build_proof;
namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (weather setup chain)";
constexpr const char *SETUP_FN = "Sess_GameWeatherSystem_SetupSession";
constexpr const char *BUILD_FN = "Sess_Tod_BuildSessionDataset_Thunk";
constexpr const char *SEED_FN = "Sess_GameWeatherSystem_UpdateRandomSeed";
constexpr const char *const BINDINGS[] = {SETUP_FN, BUILD_FN, SEED_FN};
constexpr bp::Proof LAYOUT{"the weather setup layout (GameWeatherSystem seeds and TOD owner, the TimeOfDaySystem "
                           "datasets, type ranges and build flags, WeatherSessionSettings)",
                           bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};
constexpr bp::Gate GATE{"[weather] setup chain", &LAYOUT, BINDINGS};
constexpr uint32_t TIME_TRIAL = 0x12;
constexpr uint32_t DATASET_CAP = 38;

g::Sess_GameWeatherSystem_SetupSession_t *setup_orig = nullptr;
g::Sess_Tod_BuildSessionDataset_Thunk_t *build_fn = nullptr;
g::Sess_GameWeatherSystem_UpdateRandomSeed_t *seed_fn = nullptr;

runtime::loader::MixinGate mixin_gate;
bool wanted = false;
bool test_on = false;
wx::TestScript test_script;

bool chain_on()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_WEATHER_CHAIN", v, sizeof v);
    return n && n < sizeof v && !strcmp(v, "1");
}

void load_test_script()
{
    char v[1024] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_WEATHER_TEST", v, sizeof v);
    if (!n)
        return;
    if (n >= sizeof v) {
        swofty::log("[weather] SWOFTY_WEATHER_TEST ignored: longer than %u characters", unsigned(sizeof v - 1));
        return;
    }
    std::string why = wx::parse_test_script(v, test_script);
    if (!why.empty()) {
        swofty::log("[weather] SWOFTY_WEATHER_TEST ignored: %s", why.c_str());
        return;
    }
    test_on = true;
    swofty::log("[weather] SWOFTY_WEATHER_TEST=%s: set on every session.weather.setup before the handlers run", v);
}

void apply_test_script(w::events::Setup &e)
{
    if (!test_on)
        return;
    if (test_script.has_seed)
        w::set_seed(e, test_script.seed);
    for (unsigned d = 0; d < wx::DATASETS; d++) {
        if (!(test_script.mask & (1u << d)))
            continue;
        w::Slot slots[wx::MAX_STATES] = {};
        for (unsigned i = 0; i < test_script.counts[d]; i++) {
            const wx::Slot &t = test_script.slots[d][i];
            slots[i] = {w::Kind(t.kind), {}, t.duration_s, t.transition_s, t.intensity};
        }
        w::script(e, w::Dataset(d), std::span<const w::Slot>(slots, test_script.counts[d]));
    }
}

uint32_t built_states(const g::WeatherDataset &d, wx::BuiltState (&out)[DATASET_CAP])
{
    uint32_t n = d.state_total > DATASET_CAP ? DATASET_CAP : d.state_total;
    for (uint32_t i = 0; i < n; i++)
        out[i] = {uint8_t(d.states[i].type), d.states[i].end, d.states[i].transition, d.states[i].intensity};
    return n;
}

void call_build(g::TimeOfDayOwner *owner, const wx::Build &b, float len, unsigned d)
{
    const uint8_t random[wx::MAX_STATES] = {};
    build_fn(owner, b.type, b.hold, b.trans, random, b.n, len, uint16_t(0x100 | d), 1);
}

// Rebuilds dataset d from its script. "" when the game's dataset holds the script, else why it does not.
std::string rebuild(g::GameWeatherSystem *ws, unsigned d, const w::Slot *slots, unsigned count, std::vector<std::string> &lines)
{
    g::TimeOfDayOwner *owner = ws->tod_owner;
    g::TimeOfDaySystem *tod = ws->tod;
    if (!owner || !tod || !mem::readable(owner, sizeof *owner) || owner->tod != tod || !mem::readable(tod, sizeof *tod))
        return "no TimeOfDaySystem owned by this weather system";
    wx::TodFlags flags{tod->busy != 0,
                       tod->ready_a != 0,
                       tod->ready_b != 0,
                       tod->override_active != 0,
                       tod->random_override != 0,
                       tod->scripted_scenario != 0};
    if (const char *why = wx::rebuild_gate(flags); *why)
        return why;
    float len = wx::session_length(tod->dataset_length[d], d);
    wx::Slot script[wx::MAX_STATES];
    unsigned n = count < wx::MAX_STATES ? count : wx::MAX_STATES;
    for (unsigned i = 0; i < n; i++)
        script[i] = {uint8_t(slots[i].kind), slots[i].duration_s, slots[i].transition_s, slots[i].intensity};
    wx::Build b = wx::build_of(script, n, len);
    if (b.dropped)
        lines.push_back(std::to_string(b.dropped) + " slot(s) dropped, no room for a transition and a hold before "
                                                    "the session ends");
    wx::TypeRange ranges[wx::MAX_STATES];
    for (unsigned t = 0; t < wx::MAX_STATES; t++)
        ranges[t] = {tod->type_ranges[t].intensity_min, tod->type_ranges[t].intensity_max};
    for (std::string &c : wx::clamp(b, ranges))
        lines.push_back(std::move(c));
    std::string lacking = wx::unsupported_state(b, ranges);
    if (!lacking.empty())
        return "refused: this track has no " + lacking + "; the game's forecast is kept";
    std::string bad = wx::validate(b, len);
    if (!bad.empty())
        return "refused: " + bad + "; the game's forecast is kept";

    g::WeatherDataset &set = tod->datasets[d];
    wx::BuiltState game[DATASET_CAP];
    uint32_t game_n = built_states(set, game);
    int game_first = game_n ? int(game[0].type) : -1;
    uint8_t exact = tod->exact_timing;
    tod->exact_timing = 1;
    call_build(owner, b, len, d);
    wx::BuiltState now[DATASET_CAP];
    uint32_t now_n = built_states(set, now);
    if (!wx::dataset_finite(now, now_n)) {
        lines.push_back("the game built a non-finite or out-of-range state from the script; rebuilding one plain state");
        b = wx::plain_build(game_first, len);
        call_build(owner, b, len, d);
        now_n = built_states(set, now);
    }
    tod->exact_timing = exact;
    bool match = wx::holds(now, now_n, b);
    if (match)
        for (uint32_t i = 0; i < b.n; i++)
            if (b.intensity[i] >= 0)
                set.states[i].intensity = b.intensity[i];
    ws->dataset_dynamic[d] = 0;
    return match ? "" : "MISMATCH: the game's dataset does not hold the scripted states";
}

void force_seed(g::GameWeatherSystem *ws, uint32_t forced, uint32_t base)
{
    ws->forced_seed = forced;
    seed_fn(ws, base, ws->weekend);
}

void setup_entry(void *ws_p, void *settings_p, void *config)
{
    auto *ws = static_cast<g::GameWeatherSystem *>(ws_p);
    auto *set = static_cast<const g::WeatherSessionSettings *>(settings_p);
    if (!ws || !set || !mem::readable(ws, sizeof *ws) || !mem::readable(set, sizeof *set)) {
        setup_orig(ws_p, settings_p, config);
        return;
    }
    w::events::Setup e{};
    e.size = sizeof e;
    e.payload_version = w::events::Setup::version;
    e.session_type = set->session_type;
    e.game_scripted = set->exact_timing != 0;
    e.time_trial = set->session_type == TIME_TRIAL;
    uint32_t forced = ws->forced_seed, base = ws->base_seed;
    e.seed_forced = forced != 0;
    e.seed = forced ? forced : base;
    apply_test_script(e);
    swofty_core::hooks::events::weather_setup(e);

    wx::Seed rule = wx::seed_rule(forced, e.set_seed != 0, e.seed);
    if (rule.action == wx::SeedAction::force)
        force_seed(ws, rule.value, base);
    if (rule.action == wx::SeedAction::force || (rule.action == wx::SeedAction::keep_forced && e.set_seed))
        swofty::log("[weather] session type %u: %s", e.session_type, wx::seed_line(rule, "a handler").c_str());

    setup_orig(ws_p, settings_p, config);

    uint8_t mask = wx::core_rebuilds(e.script_mask, e.game_scripted, e.time_trial);
    if (e.script_mask && !mask)
        swofty::log("[weather] session type %u: %s; the scripts are not applied", e.session_type,
                    e.time_trial ? "a Time Trial setup" : "the game scripts this setup");
    for (unsigned d = 0; d < wx::DATASETS; d++) {
        if (!(mask & (1u << d)))
            continue;
        std::vector<std::string> lines;
        std::string why = rebuild(ws, d, e.slots[d], e.counts[d], lines);
        for (const std::string &l : lines)
            swofty::log("[weather] %s: %s", wx::DATASET_KEYS[d], l.c_str());
        if (why.empty())
            swofty::log("[weather] %s: rebuilt from the script (%u state(s))", wx::DATASET_KEYS[d], unsigned(e.counts[d]));
        else
            swofty::log("[weather] %s: not applied: %s", wx::DATASET_KEYS[d], why.c_str());
    }
    if (rule.release_after)
        force_seed(ws, 0, base);
}

std::vector<runtime::CoreMixins> weather_mixins()
{
    std::vector<runtime::CoreMixins> out;
    if (mixin_gate.offered() || !runtime::code_ready())
        return out;
    mixin_gate.offer();
    if (!chain_on())
        return out;
    if (!runtime::holds(GATE))
        return out;
    setup_orig = nullptr;
    build_fn = reinterpret_cast<g::Sess_Tod_BuildSessionDataset_Thunk_t *>(swofty::api->resolve(BUILD_FN));
    seed_fn = reinterpret_cast<g::Sess_GameWeatherSystem_UpdateRandomSeed_t *>(swofty::api->resolve(SEED_FN));
    if (!build_fn || !seed_fn)
        return out;
    wanted = true;
    load_test_script();
    out.push_back({MIXIN, {{SETUP_FN, SWOFTY_MIXIN_WRAP, wx::PRIORITY, (void *)&setup_entry, (void **)&setup_orig}}});
    return out;
}

void weather_commit()
{
    if (!wanted || !mixin_gate.settle())
        return;
    swofty::log("[weather] setup chain %s",
                runtime::loader::install_state(runtime::core_mixins_active(MIXIN),
                                               "armed (SWOFTY_WEATHER_CHAIN=1): session.weather.setup fires, below "
                                               "Career Mode Editor's weather wrap"));
}

runtime::loader::FunctionSet weather_core_set{runtime::loader::CoreSetId::weather_chain, weather_mixins, weather_commit};

}
