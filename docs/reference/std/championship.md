---
title: Championship standings
description: The loaded career's drivers' and constructors' championships, from any thread, with swofty/standings.hpp.
---

# Championship standings

The standings library gives the loaded career's current season: the drivers' and the constructors' championships in the game's order with the game's totals, each row's wins, podiums and points per round, and where the season is. The core reads them about once a second while a Driver Career or My Team career is loaded, and every call copies from that snapshot. The calls are cheap and work from any thread, a paint callback included. A title card, a points chart over the season or a "P3 in the championship" strap are a few lines.

```cpp [native/src/champion.cpp]
#include <swofty/events.hpp>
#include <swofty/mixin.hpp>
#include <swofty/standings.hpp>

namespace standings = swofty::standings;

static void frame(double, void *)
{
    std::optional<standings::Season> s = standings::season();
    if (!s)
        return; // no career loaded, Grand Prix, or a core without the standings
    if (standings::complete(*s)) {
        std::vector<standings::Row> d = standings::drivers();
        if (!d.empty())
            swofty::log("[champion] driver %u (team %u, #%u) is champion with %d points and %d wins", d[0].driver,
                        d[0].team, d[0].race_number, d[0].points, d[0].wins);
    }
}

// points after each round of the top five drivers, for a chart
static void chart(const standings::Season &s)
{
    std::vector<standings::Row> d = standings::drivers();
    for (size_t i = 0; i < d.size() && i < 5; i++)
        for (int round = 1; round <= s.scored; round++)
            swofty::log("[chart] P%d after round %d: %d", d[i].position, round, standings::points_after(d[i], round));
}

SWOFTY_ON_LOAD
{
    swofty::events::on_frame(frame);
}
```

- **Header:** `swofty/standings.hpp`, namespace `swofty::standings`; host test fake `swofty/fake_standings.hpp`
- **Table:** the core export `SwoftyCoreStandings` (`SwoftyStandings`, table version 1); `SWOFTY_API_VERSION` is unchanged
- **Feature:** `career`, stable
- **Thread:** any thread; every call copies the core's snapshot

```cpp
namespace swofty::standings {

using Row = SwoftyStandingsRow;
using Season = SwoftyStandingsSeason;

bool available();                             // the core serves the standings on this build
std::optional<Season> season();               // where the season is
std::vector<Row> drivers();                   // the drivers' championship, leader first
std::vector<Row> constructors();              // the constructors' championship, leader first

bool is_player(const Row &r);                 // the player's driver or team
bool has_team(const Row &r);
bool complete(const Season &s);               // the last round's main race has results
int32_t points_in(const Row &r, int round);   // round is 1-based
int32_t points_after(const Row &r, int round);
const Row *find_driver(const std::vector<Row> &rows, uint32_t driver);
const Row *find_team(const std::vector<Row> &rows, uint32_t team);

}
```

## What a call answers

Every call answers empty with no career loaded (Grand Prix and the other modes included), before the core's first read of a career, on a core without the standings and on a build the read is not proven on. Steam 1.0.145, EA App 1.0.145 and the KB build are proven. While the career is busy (a line-up update, the step between two seasons) the calls keep answering the last read.

```cpp
struct SwoftyStandingsSeason {
    uint64_t seq;            // changes whenever the snapshot does
    uint32_t mode;           // SWOFTY_CAREER_DRIVER or SWOFTY_CAREER_MY_TEAM
    int32_t year;            // the season's year as the career's content reported it, 0 when none did
    int32_t season_index;    // 0 the career's first season
    int32_t round;           // 1-based: the weekend the career is at, the next race to run
    int32_t rounds;          // the season's weekend count; round and rounds are 0 while it has no built weekends
    int32_t scored;          // rounds with points or results
    int32_t raced;           // rounds whose main race has results
    uint32_t flags;          // SWOFTY_STANDINGS_COMPLETE: the last round's main race has results
    uint32_t drivers, constructors;   // the row counts
};

struct SwoftyStandingsRow {
    uint32_t driver;         // driver stat id; SWOFTY_STANDINGS_NONE in a constructors row
    uint32_t team;           // team stat id; SWOFTY_STANDINGS_NONE when the driver has no team
    int32_t position;        // 1-based
    int32_t points;          // the game's total
    int32_t wins, podiums;   // main-race finishes in first and in the top three
    uint8_t race_number;     // 0 when it is not known, and in constructors rows
    uint8_t flags;           // SWOFTY_STANDINGS_PLAYER
    int32_t round_points[SWOFTY_STANDINGS_MAX_ROUNDS];   // the points each round gave, calendar order
};
```

- **Order and totals:** the game's own tables. The drivers' totals include the [dropped scores](/std/systems/career#progressive-s-career-mode) rule; the constructors' count every result.
- **Before the first scored session:** `drivers()` is empty and `constructors()` holds the game's season-start order, every team on 0 points.
- **When a race counts:** the game adds a race to its tables when the session results screen opens; for a simulated race that is after the rewards screens. The next read, within a second, has it.
- **Rounds:** `round_points` holds a round's points sessions added up, the sprint included. A round past `SWOFTY_STANDINGS_MAX_ROUNDS` (32) counts in `points` only.
- **Wins and podiums:** finishes in the round's main race with a status that scores. The main race is the last race session of the weekend, so a sprint weekend's sprint is neither a win nor a podium.
- **Driver rows:** `team` and `race_number` are the driver's career row while he has a seat, else the team and number of the last round he raced.
- **Constructors rows:** a team's wins and podiums are those of the drivers it fielded in each round, from the weekend's own line-up, so a driver who changes team mid-season leaves his earlier podiums with his old team.
- **Stat ids:** `driver` and `team` are the game's stat ids, the upper 16 bits of its keys: the `stat_id` of the `entrants.drivers` and `entrants.teams` rows, as the [career operations](/std/systems/career) use them. For a stock driver the live state's `driver_id` is the same id, so a row joins the session's car with `live::car_detail(v)->driver_id == row.driver`.

The core logs each change in the number of raced rounds, with both tables:

```text
[standings] season 0 (year 2026) round 4 of 24: 3 round(s) scored, 3 raced; main races 15,15,16,15,...
[standings] drivers: P1 d9 t3 #1 68 (2w 3p), P2 d7 t5 #44 50 (1w 3p), ...
[standings] constructors: P1 t3 102 (2w 4p), P2 t5 77 (1w 3p), ...
```

## Test it on the host

`swofty/fake_standings.hpp` installs a fake core table, so the code above runs without the game and an element's preview has a season to draw.

```cpp
#include <swofty/fake_standings.hpp>

swofty::fake::Standings fake;
fake.season().round = 3;
fake.season().rounds = 24;
fake.season().scored = fake.season().raced = 2;
fake.add_driver(9, 3, 1, {25, 18}).wins = 1;      // driver stat 9, team stat 3, #1: 43 points
fake.add_driver(7, 5, 44, {18, 25}).wins = 1;
fake.add_constructor(3, {40, 33});
fake.add_constructor(5, {33, 40});
fake.install();

assert(swofty::standings::drivers().front().points == 43);
assert(swofty::standings::points_after(swofty::standings::drivers()[1], 1) == 18);
fake.set_loaded(false);                          // as with no career loaded
assert(!swofty::standings::season());
```

## The C table

```c
typedef struct SwoftyStandings {
    uint32_t version;                                                     /* SWOFTY_STANDINGS_VERSION, 1 */
    int (*available)(void);
    int (*season)(SwoftyStandingsSeason *out);
    int (*drivers)(SwoftyStandingsRow *out, uint32_t cap, uint32_t *count);
    int (*constructors)(SwoftyStandingsRow *out, uint32_t cap, uint32_t *count);
} SwoftyStandings;

const SwoftyStandings *SwoftyCoreStandings(void);   /* swofty_core.dll export */
```

`drivers` and `constructors` copy up to `cap` rows into `out` and set `*count` to the row count; `out` may be NULL with `cap` 0 to ask for the count. Each call returns `SWOFTY_ASSET_LATE` with no career loaded and before the first read, and `SWOFTY_ASSET_UNAVAILABLE` on a build the read is not proven on.

## What it reads

The core reads the season at the championship's `season_index` on the flow thread:

- **Drivers:** the season's standings (`F1SeasonStandings`, Season +0x2a8), as [Standings](./standings) reads them.
- **Constructors:** the same model at Season +0x3b8: the latest session of the cumulative table (+0x40) for the order and the totals, the points sessions (+0x20) for each round, and the season-start order (+0xd8) while nothing is scored.
- **Who raced where:** the career's entrant rows for the seats now, and each weekend's own line-up copy (+0x20, rows of 0x88 bytes) for the team a driver raced for in that round. The weekend's sessions name its main race (the last of ids 15 to 17).

Every read is a guarded copy (`swofty::mem::read`); a vector that does not read whole keeps the last snapshot. The constructors' offsets and the weekend's line-up copy are read by instructions equal on each proven build (`re/tools/sigport.py --witness`; see `career_ops.cpp`).
