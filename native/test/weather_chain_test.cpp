// Host unit test for the weather setup chain's order rules (core/weather/chain.hpp): Career Mode Editor's wrap above
// the core's, the seed forced above the core kept, a handler's seed forced for one build and released, and who owns
// each dataset with and without a Career Mode Editor plan, and the TimeOfDaySystem gates of a rebuild. The chain is
// played as the mixin chain runs it: the higher priority first on the way in and last on the way out. Then the
// build rules Career Mode Editor's plans are held to: building a script, validating it, clamping intensities into the
// track's type ranges, refusing a wet state the track cannot draw, and checking the dataset the game built.
#include "core/weather/chain.hpp"

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

namespace wx = swofty_core::weather;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Ce {
    bool present = false;
    uint32_t pin = 0;
    uint8_t plan = 0;
};

struct Addon {
    bool seed = false;
    uint32_t value = 0;
    uint8_t script = 0;
};

struct Result {
    uint32_t seed_used = 0, forced_after = 0;
    wx::Owner owner[wx::DATASETS] = {};
    wx::Seed rule;
};

// One SetupSession through the chain: Career Mode Editor (when present) pushes its pin before calling down and
// rebuilds its planned datasets after the call returns; the core applies the seed rule, the game builds with
// base = forced ? forced : the game's seed, then the core rebuilds the scripted datasets and releases its seed.
static Result setup(const Ce &ce, const Addon &addon, uint32_t game_seed, bool game_scripted = false, bool tt = false)
{
    uint32_t forced = 0;
    if (ce.present && ce.pin && !game_scripted && !tt)
        forced = ce.pin;
    Result r;
    r.rule = wx::seed_rule(forced, addon.seed, addon.value);
    if (r.rule.action == wx::SeedAction::force)
        forced = r.rule.value;
    r.seed_used = forced ? forced : game_seed;
    uint8_t core = wx::core_rebuilds(addon.script, game_scripted, tt);
    if (r.rule.release_after)
        forced = 0;
    uint8_t mine = ce.present && !game_scripted && !tt ? ce.plan : 0;
    for (unsigned d = 0; d < wx::DATASETS; d++)
        r.owner[d] = wx::owner_of(d, core, mine);
    r.forced_after = forced;
    return r;
}

static void test_priorities()
{
    expect(wx::CAREER_MODE_EDITOR_PRIORITY > wx::PRIORITY,
           "Career Mode Editor's wrap (priority 0) runs above the core's (-100): first in, last out");
}

static void test_career_mode_editor_plan()
{
    Ce ce{true, 0, uint8_t(1u << 5)};
    Addon addon{false, 0, uint8_t((1u << 5) | (1u << 3))};
    Result r = setup(ce, addon, 777);
    expect(r.owner[5] == wx::Owner::career_mode_editor, "the race: Career Mode Editor's plan beats the addon's script");
    expect(r.owner[3] == wx::Owner::addon, "qualifying: the addon's script applies where Career Mode Editor has no plan");
    expect(r.owner[0] == wx::Owner::game, "practice 1: neither scripts it, the game's build stays");
}

static void test_career_mode_editor_absent()
{
    Addon addon{false, 0, uint8_t((1u << 5) | (1u << 3))};
    Result r = setup(Ce{}, addon, 777);
    expect(r.owner[5] == wx::Owner::addon && r.owner[3] == wx::Owner::addon && r.owner[1] == wx::Owner::game,
           "without Career Mode Editor the addon's script applies whole");
    expect(r.seed_used == 777 && r.rule.action == wx::SeedAction::none, "no seed anywhere: the game's seed");
}

static void test_seed_forced_above()
{
    Ce ce{true, 4242, 0};
    Addon addon{true, 1990, 0};
    Result r = setup(ce, addon, 777);
    expect(r.rule.action == wx::SeedAction::keep_forced && r.seed_used == 4242,
           "a seed forced above the core (Career Mode Editor's pin) is the one the game builds with");
    expect(r.rule.dropped && !r.rule.release_after && r.forced_after == 4242,
           "the addon's seed is dropped and the core does not release a seed it did not force");
    expect(wx::seed_line(r.rule, "mod.spa-storm") == "seed 4242 forced above the core kept; mod.spa-storm's seed not applied",
           "and the line says so");
    Result same = setup(ce, Addon{true, 4242, 0}, 777);
    expect(!same.rule.dropped, "an addon asking for the seed already forced drops nothing");
}

static void test_nothing_forced()
{
    Result r = setup(Ce{true, 0, 0}, Addon{true, 1990, 0}, 777);
    expect(r.rule.action == wx::SeedAction::force && r.seed_used == 1990,
           "nothing forced: the handler's seed is forced for this build");
    expect(r.forced_after == 0, "and released after it, so the next session and a save see no forced seed");
    expect(wx::seed_line(r.rule, "mod.spa-storm") == "seed 1990 (mod.spa-storm) forced for this build, released after it",
           "the line for it");
    expect(wx::seed_rule(0, true, 0).action == wx::SeedAction::none,
           "a seed of 0 is the game's 'not forced' and asks for nothing");
}

static void test_scripted_and_time_trial()
{
    Addon addon{false, 0, 0xff};
    Result scripted = setup(Ce{true, 0, 0xff}, addon, 1, true, false);
    Result tt = setup(Ce{true, 0, 0xff}, addon, 1, false, true);
    bool game = true;
    for (unsigned d = 0; d < wx::DATASETS; d++)
        game = game && scripted.owner[d] == wx::Owner::game && tt.owner[d] == wx::Owner::game;
    expect(game, "the game's own scripted weather and a Time Trial setup are the game's alone");
}

static void test_rebuild_gate()
{
    wx::TodFlags ready{false, true, true, false, false, false};
    expect(!*wx::rebuild_gate(ready), "a ready, idle TimeOfDaySystem builds");
    wx::TodFlags t = ready;
    t.busy = true;
    expect(std::string(wx::rebuild_gate(t)).starts_with("TimeOfDaySystem busy"), "busy: no rebuild");
    t = ready;
    t.ready_b = false;
    expect(std::string(wx::rebuild_gate(t)).starts_with("TimeOfDaySystem not ready"), "not ready: no rebuild");
    t = ready;
    t.override_active = t.random_override = t.scripted_scenario = true;
    expect(std::string(wx::rebuild_gate(t)).starts_with("TimeOfDaySystem random override"),
           "an override with random set diverts to random states, tested before the scripted one");
    t.random_override = false;
    expect(std::string(wx::rebuild_gate(t)).starts_with("TimeOfDaySystem scripted override"),
           "an override with the scripted scenario set diverts to it");
    t.override_active = false;
    t.random_override = true;
    expect(!*wx::rebuild_gate(t), "the random and scripted flags without the override run the normal build");
}

static void test_build_of()
{
    wx::Slot spa[3] = {{0, 1200, 0, -1}, {2, 600, 300, -1}, {4, 0, 120, 0.8f}};
    wx::Build b = wx::build_of(spa, 3, 3600);
    expect(b.n == 3 && b.dropped == 0 && b.type[0] == 0 && b.type[1] == 2 && b.type[2] == 4,
           "dry, overcast, heavy rain: three states in order");
    expect(b.hold[0] == 1200 && b.trans[0] == 300 && b.hold[1] == 600 && b.trans[1] == 120,
           "each state holds its duration, then changes over the next slot's transition");
    expect(b.hold[2] == 3600 - 2220 && b.trans[2] == 0 && b.intensity[2] == 0.8f,
           "the last state holds to the end of the session with no transition");
    expect(wx::validate(b, 3600).empty(), "and the build validates");

    wx::Slot late[3] = {{0, 3000, 0, -1}, {3, 500, 300, -1}, {5, 600, 60, -1}};
    wx::Build l = wx::build_of(late, 3, 3600);
    expect(l.n == 2 && l.dropped == 1 && l.hold[1] == 300,
           "a change with no room for its transition and a hold before the end is dropped; the state before it holds on");

    wx::Slot many[8];
    for (unsigned i = 0; i < 8; i++)
        many[i] = {uint8_t(i % 6), 60, 30, -1};
    wx::Build m = wx::build_of(many, 8, 3600);
    expect(m.n == wx::MAX_STATES && m.dropped == 2, "past six states the rest are dropped and counted");
    expect(wx::validate(m, 3600).empty(), "six states validate");

    wx::Slot tiny[2] = {{1, 0, 0, -1}, {3, 0, 0, -1}};
    wx::Build t = wx::build_of(tiny, 2, 3600);
    expect(t.hold[0] == wx::MIN_HOLD_S && t.trans[0] == wx::MIN_TRANSITION_S,
           "a zero duration or transition is raised to the 1 s minimum");
    expect(wx::build_of(spa, 0, 3600).n == 0 && !wx::validate(wx::build_of(spa, 0, 3600), 3600).empty(),
           "no slots: no build, and it does not validate");
}

static void test_validate()
{
    wx::Build ok = wx::plain_build(2, 3600);
    expect(wx::validate(ok, 3600).empty() && ok.type[0] == 2, "the plain build keeps the game's first type");
    expect(wx::plain_build(9, 3600).type[0] == 0, "and falls back to clear without one");
    auto bad = [&](auto edit, const char *starts, const std::string &what) {
        wx::Build b = ok;
        b.n = 2;
        b.type[1] = 3;
        b.hold[0] = 1000;
        b.trans[0] = 60;
        b.hold[1] = 2540;
        b.trans[1] = 0;
        edit(b);
        std::string why = wx::validate(b, 3600);
        expect(*starts ? why.starts_with(starts) : why.empty(), what + (why.empty() ? "" : " (" + why + ")"));
    };
    bad([](wx::Build &) {}, "", "a two-state build fits");
    bad([](wx::Build &b) { b.type[1] = 6; }, "state 1 has type 6", "a type past storm");
    bad([](wx::Build &b) { b.hold[0] = 0.5f; }, "state 0 hold", "a hold under 1 s");
    bad([](wx::Build &b) { b.trans[1] = 5; }, "state 1 transition", "a transition after the last state");
    bad([](wx::Build &b) { b.trans[0] = NAN; }, "state 0 transition", "a transition that is not a number");
    bad([](wx::Build &b) { b.intensity[1] = 1.5f; }, "state 1 intensity", "an intensity above 1");
    bad([](wx::Build &b) { b.hold[1] = 4000; }, "states add up to", "states past the session's end");
    bad([](wx::Build &b) { b.n = 7; }, "7 states", "more than six states");
    expect(!wx::validate(ok, NAN).empty(), "a session length that is not a number");
    expect(wx::session_length(5400, 3) == 5400 && wx::session_length(0, 3) == 3600 && wx::session_length(NAN, 5) == 7200,
           "the game's length when plausible, else an hour, or two for a race");
}

static void test_ranges()
{
    wx::TypeRange ranges[wx::MAX_STATES] = {{0, 0}, {0, 0.4f}, {0.2f, 0.6f}, {0.1f, 0.5f}, {0, 0}, {0.6f, 1}};
    wx::Slot wet[2] = {{3, 600, 0, -1}, {4, 0, 120, -1}};
    wx::Build b = wx::build_of(wet, 2, 3600);
    expect(wx::unsupported_state(b, ranges) == "heavy_rain", "heavy rain on a track whose range cannot draw it");
    wx::Slot dry[2] = {{0, 600, 0, -1}, {2, 0, 120, -1}};
    wx::Build d = wx::build_of(dry, 2, 3600);
    expect(wx::unsupported_state(d, ranges).empty(), "dry states are never refused");

    wx::Slot set[3] = {{2, 600, 0, 0.9f}, {3, 600, 60, 0.3f}, {5, 0, 60, 0.2f}};
    wx::Build c = wx::build_of(set, 3, 3600);
    std::vector<std::string> lines = wx::clamp(c, ranges);
    expect(c.intensity[0] == 0.6f && c.intensity[1] == 0.3f && c.intensity[2] == 0.6f,
           "intensities move into their type's range; one already in it stays");
    expect(lines.size() == 2 && lines[0] == "state 0 overcast intensity 0.90 -> 0.60 (game range 0.20..0.60)",
           "one line per changed state");
    wx::TypeRange broken[wx::MAX_STATES] = {{0, 0}, {0, 0}, {NAN, 1}, {0, 1}, {0, 1}, {0, 1}};
    wx::Build u = wx::build_of(set, 3, 3600);
    std::vector<std::string> back = wx::clamp(u, broken);
    expect(u.intensity[0] == -1 && back.size() == 1 && back[0].find("the game's draw") != std::string::npos,
           "an unusable range hands the state back to the game's draw");
}

static void test_dataset_checks()
{
    wx::BuiltState good[2] = {{0, 1200, 300, 0}, {4, 3600, 0, 0.7f}};
    wx::BuiltState nan[2] = {{0, 1200, NAN, 0}, {4, 3600, 0, 0.7f}};
    wx::BuiltState high[2] = {{0, 1200, 300, 0}, {4, 3600, 0, 1.2f}};
    expect(wx::dataset_finite(good, 2) && !wx::dataset_finite(nan, 2) && !wx::dataset_finite(high, 2) &&
               wx::dataset_finite(high, 2, false),
           "a built dataset is checked finite, and in range unless asked not to");
    wx::Slot two[2] = {{0, 1200, 0, -1}, {4, 0, 300, -1}};
    wx::Build b = wx::build_of(two, 2, 3600);
    wx::BuiltState other[2] = {{0, 1200, 300, 0}, {3, 3600, 0, 0.5f}};
    expect(wx::holds(good, 2, b) && !wx::holds(other, 2, b) && !wx::holds(good, 1, b),
           "the game's dataset holds the build's states in order, or the rebuild is a mismatch");
}

static void test_test_script()
{
    wx::TestScript t;
    std::string why = wx::parse_test_script("seed=1990;r=clear:1200,overcast:600:300,heavy_rain:0:120:0.8", t);
    expect(why.empty() && t.has_seed && t.seed == 1990 && t.mask == (1u << 5) && t.counts[5] == 3 && t.slots[5][0].kind == 0 &&
               t.slots[5][0].duration_s == 1200 && t.slots[5][0].intensity < 0 && t.slots[5][1].kind == 2 &&
               t.slots[5][1].transition_s == 300 && t.slots[5][2].kind == 4 && t.slots[5][2].intensity == 0.8f,
           "a test script sets the seed and the race's three slots: " + why);
    expect(wx::parse_test_script("q=storm:600;p1=light_rain:0", t).empty() && !t.has_seed && t.mask == ((1u << 3) | (1u << 0)) &&
               t.counts[3] == 1 && t.counts[0] == 1,
           "a test script scripts each dataset key it names");
    expect(!wx::parse_test_script("seed=0", t).empty() && !wx::parse_test_script("seed=x", t).empty() &&
               !wx::parse_test_script("fp1=clear:60", t).empty() && !wx::parse_test_script("r=snow:60", t).empty() &&
               !wx::parse_test_script("r=clear", t).empty() && !wx::parse_test_script("r=clear:6o", t).empty() &&
               !wx::parse_test_script("r=clear:1,clear:1,clear:1,clear:1,clear:1,clear:1,clear:1", t).empty() &&
               !wx::parse_test_script("r", t).empty(),
           "a test script with a bad seed, key, kind, field or slot count is refused");
    expect(wx::parse_test_script("", t).empty() && !t.mask && !t.has_seed, "an empty test script sets nothing");
}

int main()
{
    test_priorities();
    test_career_mode_editor_plan();
    test_career_mode_editor_absent();
    test_seed_forced_above();
    test_nothing_forced();
    test_scripted_and_time_trial();
    test_rebuild_gate();
    test_build_of();
    test_validate();
    test_ranges();
    test_dataset_checks();
    test_test_script();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
