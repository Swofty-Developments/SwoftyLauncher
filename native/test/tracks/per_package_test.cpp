// Host test for per_package.hpp and the box fuel plan of data_session.cpp (fuel_plan.hpp): entry targets by track,
// venue or the id's last code, a variant package inheriting its base's entries with its own winning, and the
// writes and put-back order of the fuel plan over a fake data set. run_track_lists_test.sh builds and runs it.
#include "fixture.hpp"

#include "../../src/core/apply/tracks/fuel_plan.hpp"

int main()
{
    trk::Base base = cs_base();
    std::vector<std::pair<std::string, Value>> recs = {
        {"cs/f1_2016/hockenheim", Value::object({{"venue", Value("venue/albert_park")},
                                                {"fuel", Value::object({{"burn_per_lap", Value(1.9)}})}})},
        {"cs/f1_2016/monaco", Value::object({{"offered", Value(false)}})},
        {"cs/f1_2016/sochi", Value::object({{"venue", Value("venue/sochi?")}})},
        {"cs/f1_2016/okay", Value::object({{"track", Value("mod.okayama/okayama")}, {"order", Value(2)}})},
        {"cs/f1_2016_v10/melbourne", Value::object({{"fuel", Value::object({{"burn_per_lap", Value(2.1)}})}})},
        {"cs/f1_2016_v10/monaco", Value::object({{"offered", Value(true)}, {"fuel", Value()}})}};
    trk::Entries e = trk::resolve_entries(base, recs);
    expect(e.by_track.size() == 3, "three targets: Melbourne, Monaco and Okayama");
    expect(e.of("game/melbourne") && e.of("game/melbourne")->id == "cs/f1_2016_v10/melbourne" &&
               e.of("game/melbourne")->burn && *e.of("game/melbourne")->burn > 2.09f,
           "the variant's own entry for a target wins over its base's (venue/albert_park resolves to Melbourne)");
    expect(e.of("game/monaco") && e.of("game/monaco")->offered && !e.of("game/monaco")->burn,
           "the variant offers Monaco again; a null fuel writes nothing");
    expect(e.of("mod.okayama/okayama") && e.of("mod.okayama/okayama")->order == 2, "an entry by track id keeps its order");
    expect(e.lines.size() == 1 && e.lines[0] == "[tracks] cs/f1_2016/sochi: no track for venue/sochi? (optional); nothing applied",
           "an entry whose venue has no track is skipped with a line");
    expect(trk::entry_of_package("cs/f1_2016/monaco", "cs/f1_2016") && !trk::entry_of_package("cs/f1_2016_v10/monaco", "cs/f1_2016") &&
               !trk::entry_of_package("cs/f1_2016", "cs/f1_2016") && !trk::entry_of_package("cs/f1_2016/a/b", "cs/f1_2016"),
           "a package's entries are the ids one level under it");

    std::vector<std::pair<std::string, Value>> all = {
        {"cs/f1_2016/bahrain", Value::object({{"package", Value("cs/f1_2016")}, {"order", Value(1)}})},
        {"ig9.checks/monaco", Value::object({{"package", Value("cs/f1_2017")}, {"fuel", Value::object({{"burn_per_lap", Value(2.5)}})}})},
        {"ig9.checks/melbourne", Value::object({{"package", Value("cs/f1_2016")}, {"offered", Value(false)}})},
        {"cs/f1_2016_v10/melbourne", Value::object({{"package", Value("cs/f1_2016_v10")}, {"offered", Value(true)}})}};
    std::vector<std::pair<std::string, Value>> got = trk::chain_records(all, {"cs/f1_2017"});
    expect(got.size() == 1 && got[0].first == "ig9.checks/monaco",
           "a record of another addon whose package field names the box is one of its entries");
    trk::Entries other = trk::resolve_entries(base, got);
    expect(other.of("game/monaco") && other.of("game/monaco")->burn && *other.of("game/monaco")->burn > 2.49f,
           "its target is the base track its key names, with its fuel");
    got = trk::chain_records(all, {"cs/f1_2016_v10", "cs/f1_2016"});
    expect(got.size() == 3 && got[0].first == "cs/f1_2016/bahrain" && got[1].first == "ig9.checks/melbourne" &&
               got[2].first == "cs/f1_2016_v10/melbourne",
           "the base package's entries (its own and others naming it) come before the variant's");
    trk::Entries chained = trk::resolve_entries(base, got);
    expect(chained.of("game/melbourne")->offered && chained.of("game/bahrain")->order == 1,
           "the variant's own entry wins over another addon's entry for its base");
    expect(trk::entry_for("x/y", Value::object({{"package", Value("cs/f1_2016")}}), "cs/f1_2016") &&
               !trk::entry_for("x/y", Value::object({{"package", Value("cs/f1_2017")}}), "cs/f1_2016") &&
               trk::entry_for("cs/f1_2016/a", Value::object({}), "cs/f1_2016"),
           "an entry is a package's by its id or by its package field");

    trk::fuel::Set set;
    set.tracks.push_back({0, 0x1000, 0x10, "melbourne", {}});
    set.tracks.push_back({1, 0x2000, 0x20, "monaco", {}});
    set.tracks[0].fuel.burn = 1.3f;
    set.tracks[0].fuel.count = 1;
    set.tracks[1].fuel.burn = 1.1f;
    set.tracks[1].fuel.count = 1;
    std::vector<trk::FuelWrite> plan = trk::plan_box_fuel(base, e, {&set}, "");
    expect(plan.size() == 1 && plan[0].object == 0x1000 && plan[0].after.burn > 2.09f && plan[0].before.burn < 1.31f &&
               plan[0].entry == "cs/f1_2016_v10/melbourne",
           "the box's burn per lap goes to its track in the set the session reads");
    plan = trk::plan_box_fuel(base, e, {&set}, "monaco");
    expect(plan.empty(), "only the session's track is written when it is known");
    set.tracks[0].fuel.burn = 2.1f;
    plan = trk::plan_box_fuel(base, e, {&set}, "melbourne");
    expect(plan.empty(), "a track already at the box's value is not written");
    set.tracks[0].fuel.count = 9;
    set.tracks[0].fuel.burn = 1.3f;
    plan = trk::plan_box_fuel(base, e, {&set}, "");
    expect(plan.empty(), "a track whose fuel rows don't read as fuel is not written");

    std::vector<trk::fuel::Set> stock(5);
    auto masks = [](trk::fuel::Set &x, uint32_t modes, uint32_t seasons, uint32_t formats) {
        x.modes = modes, x.seasons = seasons, x.formats = formats;
    };
    masks(stock[0], 1u << 0 | 1u << 20, 1u << 5, 1u << 0);
    masks(stock[1], 1u << 0 | 1u << 20, 1u << 5, 1u << 1);
    masks(stock[2], 1u << 0 | 1u << 20, 1u << 7, 1u << 0);
    masks(stock[3], 1u << 0 | 1u << 20, 1u << 7, 1u << 1);
    masks(stock[4], 1u << 13, 1u << 8, 1u << 0);
    trk::Query q{20, 7, true};
    std::vector<const trk::fuel::Set *> read = trk::query_sets(stock, q);
    expect(read.size() == 2 && read[0] == &stock[2] && read[1] == &stock[3],
           "the session's query (swofty_f1, season_2025) reads f1_2025 and its sprint set");
    std::vector<const trk::fuel::Set *> into = trk::box_sets(stock, 20, &q);
    expect(into.size() == 4 && into[0] == &stock[0] && into[3] == &stock[3],
           "a box's fuel goes into every set its setup mode can read, whatever the season");
    trk::Query unused{20, 0, false};
    expect(trk::query_sets(stock, unused).front() == &stock[0] && trk::box_sets(stock, 20, &unused).size() == 4,
           "a query whose season is unused reads the first swofty_f1 set, which the box's writes cover");
    trk::Query stock26{13, 8, true};
    into = trk::box_sets(stock, 20, &stock26);
    expect(into.size() == 5 && into.back() == &stock[4], "a session query in another setup mode adds the sets it reads");
    expect(trk::box_sets(stock, 20, nullptr).size() == 4 && trk::box_sets(stock, 40, nullptr).empty(),
           "at the box press, without a query: the setup mode's sets; an out-of-range mode: none");
    expect(trk::set_masks({&stock[2]}) == "modes 0x100001 seasons 0x80 formats 0x1", "the log names a set by its masks");

    std::vector<trk::FuelWrite> done = {{0x1000, "melbourne", "a", {}, {}}, {0x2000, "monaco", "b", {}, {}}};
    std::vector<uintptr_t> order;
    for (auto it = done.rbegin(); it != done.rend(); ++it)
        order.push_back(it->object);
    expect(order == std::vector<uintptr_t>{0x2000, 0x1000}, "writes are put back in reverse order");

    printf("%s\n", fails ? "per_package_test: FAILED" : "per_package_test: all passed");
    return fails ? 1 : 0;
}
