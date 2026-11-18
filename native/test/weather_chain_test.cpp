// Host unit test for the weather setup chain's order rules (core/weather/chain.hpp): Career Mode Editor's wrap above
// the core's, the seed forced above the core kept, a handler's seed forced for one build and released, and who owns
// each dataset with and without a Career Mode Editor plan, and the TimeOfDaySystem gates of a rebuild. The chain is
// played as the mixin chain runs it: the higher priority first on the way in and last on the way out.
#include "core/weather/chain.hpp"

#include <cstdio>
#include <string>

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

int main()
{
    test_priorities();
    test_career_mode_editor_plan();
    test_career_mode_editor_absent();
    test_seed_forced_above();
    test_nothing_forced();
    test_scripted_and_time_trial();
    test_rebuild_gate();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
