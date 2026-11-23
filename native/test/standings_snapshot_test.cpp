#include "core/career/standings_snapshot.hpp"

#include <swofty/fake_standings.hpp>

#include <cstdio>
#include <vector>

static int failures;

#define CHECK(cond, msg)                                                                                                         \
    do {                                                                                                                         \
        if (cond) {                                                                                                              \
            printf("ok  : %s\n", msg);                                                                                           \
        } else {                                                                                                                 \
            printf("FAIL: %s\n", msg);                                                                                           \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

using namespace career::standings;

static uint32_t key_of(uint16_t stat, uint8_t kind = 1) { return uint32_t(stat) << 16 | kind; }

static Driver driver(uint16_t stat, int32_t points, std::vector<Round> rounds)
{
    Driver d;
    d.key = key_of(stat);
    d.points = points;
    d.rounds = std::move(rounds);
    return d;
}

static Round round_of(int32_t points, std::vector<Finish> finishes) { return Round{points, std::move(finishes)}; }

// Round 1 a normal weekend (race 15), round 2 a sprint weekend (sprint 15, race 16). Driver 9 races for team 3 all
// season; driver 7 races for team 8 in round 1 and moves to team 3's sister team 5 for round 2, where he is seated now.
static Standings drivers_table()
{
    Standings s;
    s.rounds = 2;
    s.sessions = {{15}, {15, 16}};
    s.drivers.push_back(driver(9, 51, {round_of(25, {{15, 1, 0}}), round_of(26, {{15, 2, 0}, {16, 1, 0}})}));
    s.drivers.push_back(driver(7, 41, {round_of(18, {{15, 2, 0}}), round_of(23, {{15, 1, 0}, {16, 2, 0}})}));
    s.drivers.push_back(driver(4, 15, {round_of(15, {{15, 3, 0}}), round_of(0, {{15, 3, 0}, {16, 1, 3}})}));
    return s;
}

static Constructors teams_table()
{
    Constructors c;
    c.rounds = 2;
    c.teams.push_back({key_of(3, 2), 66, {40, 26}});
    c.teams.push_back({key_of(5, 2), 23, {0, 23}});
    c.teams.push_back({key_of(8, 2), 18, {18, 0}});
    return c;
}

static Lineups lineups()
{
    Lineups l;
    l.now = {{9, 3, 1}, {7, 5, 44}, {4, 3, 16}};
    l.rounds = {{{9, 3, 1}, {7, 8, 44}, {4, 3, 16}}, {{9, 3, 1}, {7, 5, 44}, {4, 3, 16}}};
    l.races = {15, 16};
    return l;
}

static Where where()
{
    Where w;
    w.mode = SWOFTY_CAREER_DRIVER;
    w.year = 2026;
    w.season_index = 1;
    w.round = 2;
    w.rounds = 2;
    w.player_driver = 7;
    w.player_team = 5;
    return w;
}

static void test_drivers()
{
    Snapshot s = snapshot(drivers_table(), teams_table(), lineups(), where());
    CHECK(s.drivers.size() == 3 && s.drivers[0].driver == 9 && s.drivers[0].position == 1 && s.drivers[0].points == 51,
          "the drivers keep the game's order and totals");
    CHECK(s.drivers[0].round_points[0] == 25 && s.drivers[0].round_points[1] == 26 && s.drivers[0].round_points[2] == 0,
          "each round's points fill round_points");
    CHECK(s.drivers[0].wins == 2 && s.drivers[0].podiums == 2, "a main-race win counts, in a sprint weekend the race");
    CHECK(s.drivers[1].wins == 0 && s.drivers[1].podiums == 2, "a sprint win is not a win");
    CHECK(s.drivers[2].wins == 0 && s.drivers[2].podiums == 1, "a finish with a status that does not score counts for nothing");
    CHECK(s.drivers[1].team == 5 && s.drivers[1].race_number == 44, "a seated driver names his team and number now");
    CHECK(s.drivers[1].flags == SWOFTY_STANDINGS_PLAYER && s.drivers[0].flags == 0, "the player's row is flagged");
}

static void test_constructors()
{
    Snapshot s = snapshot(drivers_table(), teams_table(), lineups(), where());
    CHECK(s.constructors.size() == 3 && s.constructors[0].team == 3 && s.constructors[0].driver == SWOFTY_STANDINGS_NONE &&
              s.constructors[0].points == 66 && s.constructors[0].round_points[1] == 26,
          "the constructors keep the game's order, totals and rounds");
    CHECK(s.constructors[0].wins == 2 && s.constructors[0].podiums == 3, "a team counts its drivers' wins and podiums");
    CHECK(s.constructors[2].team == 8 && s.constructors[2].podiums == 1 && s.constructors[1].podiums == 1,
          "a moved driver's podium counts for the team he raced for in that round");
    CHECK(s.constructors[1].flags == SWOFTY_STANDINGS_PLAYER, "the player's team is flagged");
}

static void test_season()
{
    Snapshot s = snapshot(drivers_table(), teams_table(), lineups(), where());
    CHECK(s.season.round == 2 && s.season.rounds == 2 && s.season.year == 2026 && s.season.season_index == 1,
          "the season says where the career is");
    CHECK(s.season.scored == 2 && s.season.raced == 2 && (s.season.flags & SWOFTY_STANDINGS_COMPLETE),
          "the last round's race having results completes the season");
    CHECK(s.season.drivers == 3 && s.season.constructors == 3, "the season counts the rows");

    Standings d = drivers_table();
    d.sessions[1] = {15};
    d.drivers[0].rounds[1].finishes.pop_back();
    Snapshot t = snapshot(d, teams_table(), lineups(), where());
    CHECK(t.season.raced == 1 && !(t.season.flags & SWOFTY_STANDINGS_COMPLETE) && t.drivers[0].wins == 1,
          "after a sprint only, the round's race has not run");
}

static void test_fallbacks()
{
    Lineups l = lineups();
    l.races.clear();
    l.now = {{9, 3, 1}, {4, 3, 16}};
    Snapshot s = snapshot(drivers_table(), teams_table(), l, where());
    CHECK(s.drivers[0].wins == 2, "with the weekends unread the round's last results session is its race");
    CHECK(s.drivers[1].team == 5 && s.drivers[1].race_number == 44, "a driver off the grid names his last round's team");

    Standings d = drivers_table();
    d.sessions = {{3}, {3, 4}};
    for (Driver &dr : d.drivers)
        for (size_t r = 0; r < dr.rounds.size(); r++)
            for (Finish &f : dr.rounds[r].finishes)
                f.session = f.session == 15 ? 3 : 4;
    Snapshot t = snapshot(d, teams_table(), lineups(), where());
    CHECK(t.drivers[0].wins == 2 && t.season.raced == 2, "results with no race session ids take the last one");
}

static void test_same_and_copy()
{
    Snapshot a = snapshot(drivers_table(), teams_table(), lineups(), where());
    Snapshot b = a;
    b.season.seq = 9;
    CHECK(same(a, b), "seq alone is no change");
    b.drivers[2].points++;
    CHECK(!same(a, b), "a changed row is a change");
    std::vector<SwoftyStandingsRow> out(2);
    uint32_t count = 0;
    CHECK(copy_rows(a.drivers, out.data(), 2, &count) == SWOFTY_ASSET_OK && count == 3 && out[1].driver == 7,
          "copy_rows fills up to cap and reports every row");
    CHECK(copy_rows(a.drivers, nullptr, 0, &count) == SWOFTY_ASSET_OK && count == 3, "a null buffer asks for the count");
    CHECK(copy_rows(a.drivers, nullptr, 1, &count) == SWOFTY_ASSET_INVALID, "a null buffer with a cap is invalid");
}

static void test_sdk_fake()
{
    namespace st = swofty::standings;
    CHECK(!st::season() && st::drivers().empty() && !st::available(), "no table answers empty");
    swofty::fake::Standings fake;
    fake.season().round = 3;
    fake.season().rounds = 24;
    fake.add_driver(9, 3, 1, {25, 18}).wins = 1;
    fake.add_driver(7, 5, 44, {18, 25}).flags = SWOFTY_STANDINGS_PLAYER;
    fake.add_constructor(3, {40, 33});
    fake.install();
    std::vector<st::Row> d = st::drivers();
    CHECK(st::available() && d.size() == 2 && d[0].points == 43 && d[1].position == 2, "the fake serves its rows");
    CHECK(st::points_in(d[0], 2) == 18 && st::points_after(d[0], 1) == 25 && st::points_after(d[0], 2) == 43 &&
              st::points_in(d[0], 0) == 0,
          "points_in and points_after read the rounds");
    CHECK(st::is_player(d[1]) && st::find_driver(d, 7) == &d[1] && !st::find_driver(d, 1), "find_driver and is_player");
    std::optional<st::Season> s = st::season();
    CHECK(s && s->round == 3 && s->drivers == 2 && s->constructors == 1 && !st::complete(*s), "the fake's season");
    CHECK(st::constructors().size() == 1 && st::find_team(st::constructors(), 3), "the fake's constructors");
    fake.set_loaded(false);
    CHECK(!st::season() && st::drivers().empty(), "a fake with no career answers empty");
    fake.uninstall();
    CHECK(!st::season(), "an uninstalled fake answers nothing");
}

int main()
{
    test_drivers();
    test_constructors();
    test_season();
    test_fallbacks();
    test_same_and_copy();
    test_sdk_fake();
    if (failures) {
        printf("%d failure(s)\n", failures);
        return 1;
    }
    printf("all standings snapshot checks passed\n");
    return 0;
}
