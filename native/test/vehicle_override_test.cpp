// Host unit test for team.vehicle_override (vehicle_override_logic.hpp): the feature flag, the when selector, the game
// modes it runs in, a package team's seats by driver, which entrant records a fill writes (one seat, every seat, the
// load-order winner of a seat two overrides name, a second fill of the same session) and when a write is put back.
// native/test/run_tests.sh vehicle_override builds and runs it.
#include "core/apply/content/vehicle_override_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace vo = swofty_core::apply::content::vehicle_override;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static vo::Entrant entrant(uintptr_t at, int64_t vehicle, uint16_t seat, uint32_t livery = 0x00070000u)
{
    vo::Entrant e;
    e.at = at;
    e.vehicle_ref = vo::ref_of(vehicle);
    e.customisation = livery;
    e.team_position = seat;
    return e;
}

static vo::Target target(const std::string &id, std::size_t order, int64_t team_vehicle, int64_t vehicle,
                         std::vector<int> seats = {})
{
    vo::Target t;
    t.id = id;
    t.order = order;
    t.team_vehicle = team_vehicle;
    t.vehicle_ref = vo::ref_of(vehicle);
    t.seats = std::move(seats);
    return t;
}

static void flag_tests()
{
    expect(vo::enabled("1") && vo::enabled("on") && vo::enabled("true"), "1, on and true turn the feature on");
    expect(!vo::enabled("") && !vo::enabled("0") && !vo::enabled("off") && !vo::enabled("yes"), "anything else leaves it off");
    expect(vo::ref_of(812) == (812u << 16) && vo::stat_of_ref(vo::ref_of(812)) == 812, "a vehicle ref is stat id << 16, kind 0");
    expect(vo::valid_stat(0) && vo::valid_stat(16382) && !vo::valid_stat(-1) && !vo::valid_stat(16383),
           "vehicle stat ids run 0-16382");
}

static void when_tests()
{
    vo::Session anderstorp_race{"grand_prix", "race", "vmknolle.tracks/anderstorp", 1978, true};
    vo::Session monaco_race{"grand_prix", "race", "game/monaco", 1978, true};
    vo::When weekend{"vmknolle.tracks/anderstorp", 1978, {}};
    expect(vo::when_matches(weekend, anderstorp_race), "track and year match: the whole weekend");
    expect(!vo::when_matches(weekend, monaco_race), "another track of the season keeps the team's car");
    vo::Session other_year = anderstorp_race;
    other_year.year = 1979;
    expect(!vo::when_matches(weekend, other_year), "another year does not match");
    vo::Session no_year = anderstorp_race;
    no_year.has_year = false;
    expect(!vo::when_matches(weekend, no_year), "a year is not matched by a session without one");
    expect(vo::when_matches(vo::When{}, monaco_race) && vo::when_matches(vo::When{}, no_year),
           "an empty when is every session of the box");
    vo::When race_only{"vmknolle.tracks/anderstorp", 0, {"race"}};
    vo::Session quali = anderstorp_race;
    quali.type = "qualifying";
    expect(vo::when_matches(race_only, anderstorp_race) && !vo::when_matches(race_only, quali),
           "session_types narrow it to the named session types");
}

static void mode_tests()
{
    expect(vo::mode_refusal("grand_prix", false, "").empty(), "Grand Prix runs overrides");
    expect(vo::mode_refusal("my_team", false, "").empty() && vo::mode_refusal("driver_career", false, "").empty(),
           "both careers run overrides while the save splice is armed");
    expect(!vo::mode_refusal("my_team", false, "the save splice is not armed").empty(),
           "a career without career.saving is refused");
    expect(!vo::mode_refusal("time_trial", false, "").empty(), "Time Trial is refused");
    expect(!vo::mode_refusal("unknown", false, "").empty() && !vo::mode_refusal("", false, "").empty(),
           "an unknown mode (every online mode maps to it) is refused");
    expect(!vo::mode_refusal("grand_prix", true, "").empty(), "a shared session (room) is refused");
    expect(!vo::mode_refusal("f1_world", false, "").empty() && !vo::mode_refusal("split_screen", false, "").empty(),
           "the other modes are refused");
}

static void seat_tests()
{
    std::vector<std::string> lotus = {"vmknolle.season-1981/de_angelis", "vmknolle.season-1981/mansell"};
    std::vector<int> seats;
    std::string why;
    expect(vo::seats_of(lotus, {}, seats, why) && seats.empty(), "no drivers: every seat");
    expect(vo::seats_of(lotus, {"vmknolle.season-1981/mansell"}, seats, why) && seats == std::vector<int>{1},
           "one driver: that driver's seat");
    expect(vo::seats_of(lotus, {"vmknolle.season-1981/mansell", "vmknolle.season-1981/mansell"}, seats, why) &&
               seats == std::vector<int>{1},
           "a driver named twice is one seat");
    expect(!vo::seats_of(lotus, {"vmknolle.season-1981/piquet"}, seats, why) && why.find("piquet") != std::string::npos,
           "a driver the team does not name is refused");
}

static void plan_tests()
{
    // Brabham's BT46 (stat 300) on seats 0 and 1, a Lotus (stat 310) on seats 0 and 1; the BT46B is stat 301.
    std::vector<vo::Entrant> grid = {entrant(0x1000, 300, 0), entrant(0x1088, 300, 1), entrant(0x1110, 310, 0),
                                     entrant(0x1198, 310, 1)};
    vo::Plan all = vo::plan({target("vmknolle.season-1978/f78_brabham_bt46b_anderstorp", 0, 300, 301)}, grid, {});
    expect(all.writes.size() == 2 && all.writes[0].at == 0x1000 && all.writes[1].at == 0x1088,
           "every seat of the team: both Brabham records, no Lotus");
    expect(all.writes[0].vehicle_before == vo::ref_of(300) && all.writes[0].vehicle_after == vo::ref_of(301),
           "the vehicle ref goes from the team's car to the override's");
    expect(all.writes[0].livery_before == 0x00070000u && all.writes[0].livery_after == vo::NO_KEY,
           "the livery key goes to none (the vehicle's own livery) and the old key is kept");

    vo::Plan one = vo::plan({target("lotus88", 0, 310, 311, {1})}, grid, {});
    expect(one.writes.size() == 1 && one.writes[0].at == 0x1198, "one driver: only that seat's record");

    vo::Plan none = vo::plan({target("other_box", 0, 999, 301)}, grid, {});
    expect(none.writes.empty(), "a team whose car is not in the session writes nothing");

    vo::Plan both = vo::plan({target("early", 0, 300, 301), target("late", 1, 300, 302, {0})}, grid, {});
    expect(both.writes.size() == 2 && both.writes[0].id == "late" && both.writes[0].vehicle_after == vo::ref_of(302) &&
               both.writes[1].id == "early",
           "two overrides on one seat: the later in load order wins it, the other keeps its other seat");
    expect(both.lines.size() == 1 && both.lines[0].find("late wins") != std::string::npos &&
               both.lines[0].find("early") != std::string::npos,
           "the seat's conflict is logged with the winner and the loser");

    // A second fill of the same session: the records already hold the writes, so nothing new is written.
    std::vector<vo::Entrant> after = grid;
    for (auto &w : all.writes)
        for (auto &e : after)
            if (e.at == w.at) {
                e.vehicle_ref = w.vehicle_after;
                e.customisation = w.livery_after;
            }
    vo::Plan again = vo::plan({target("vmknolle.season-1978/f78_brabham_bt46b_anderstorp", 0, 300, 301)}, after, all.writes);
    expect(again.writes.empty() && again.kept == 2, "a second fill writes nothing and counts the kept writes");

    // A record the game rebuilt since (the team's car again at the same address) is written again.
    std::vector<vo::Entrant> rebuilt = after;
    rebuilt[0].vehicle_ref = vo::ref_of(300);
    rebuilt[0].customisation = 0x00070000u;
    vo::Plan fresh = vo::plan({target("vmknolle.season-1978/f78_brabham_bt46b_anderstorp", 0, 300, 301)}, rebuilt, all.writes);
    expect(fresh.writes.size() == 1 && fresh.writes[0].at == 0x1000 && fresh.kept == 1,
           "a record that lost the write is written again, the other is kept");
}

static void restore_tests()
{
    vo::Write w{0x1000, vo::ref_of(300), vo::ref_of(301), 0x00070000u, vo::NO_KEY, "x"};
    expect(vo::restorable(w, vo::ref_of(301), vo::NO_KEY), "a record holding the write is put back");
    expect(!vo::restorable(w, vo::ref_of(300), vo::NO_KEY), "a record the game changed since is left");
    expect(!vo::restorable(w, vo::ref_of(301), 0x00080000u), "a record whose livery key changed since is left");
}

int main()
{
    flag_tests();
    when_tests();
    mode_tests();
    seat_tests();
    plan_tests();
    restore_tests();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
