---
title: Standings
description: The loaded career's drivers' championship from swofty/systems/career.hpp career::standings(), in the game's order with each driver's rounds.
---

# Standings

`career::standings()` reads the drivers' championship of the loaded career's current season. The drivers come in the game's order with the game's totals, the [dropped scores](/std/systems/career#progressive-s-career-mode) rule included. Each driver has his points and his finishes for every round. A title-decider HUD shows the gap to the leader, a custom points addon checks its totals against the game's, and a league addon writes the table out after every race.

```cpp [native/src/table.cpp]
#include <swofty/events.hpp>
#include <swofty/systems/career.hpp>

namespace career = swofty::career;

void log_table()
{
    std::optional<career::Standings> s = career::standings();
    if (!s)
        return;
    for (const career::StandingsDriver &d : s->drivers) {
        int last = d.points_in(s->rounds);
        int finish = d.rounds.empty() || d.rounds.back().finishes.empty() ? 0 : d.rounds.back().finishes.back().position;
        swofty::log("[table] P%d driver %u: %d points (%d in round %d, finished P%d)", d.position, d.driver_stat,
                    d.points, last, s->rounds, finish);
    }
}

int gap_to_leader(uint32_t driver_stat)
{
    std::optional<career::Standings> s = career::standings();
    if (!s || s->drivers.empty())
        return 0;
    const career::StandingsDriver *me = s->find(driver_stat);
    return me ? s->drivers.front().points - me->points : 0;
}

// career.hub runs on the flow thread: a = 1 when the Team HQ is entered
static void on_hub(const SwoftyEvent *ev, void *)
{
    if (ev->a)
        log_table();
}

SWOFTY_ON_LOAD
{
    swofty::events::subscribe(SWOFTY_EVENT_CAREER_HUB, on_hub);
}
```

```cpp
namespace swofty::career {

struct StandingsFinish {
    uint32_t session = 0;              // the results session's id
    int position = 0;                  // 1-based finishing position
    uint32_t status = 0;               // the game's result status: points score at 0 and 1
};

struct StandingsRound {
    int points = 0;                    // the round's sessions' points added up
    std::vector<StandingsFinish> finishes;   // one per results session he has a result in
};

struct StandingsDriver {
    uint32_t driver_stat = 0;          // the driver's stat id (career::driver_stat)
    int position = 0;                  // 1-based place in the championship
    int points = 0;                    // the game's total
    std::vector<StandingsRound> rounds;   // calendar order, Standings::rounds of them
    int points_in(int round) const;    // round is 1-based; 0 past the rounds read
};

struct Standings {
    int season_index = 0;              // as current_round gives it
    int rounds = 0;                    // rounds with points or results
    std::vector<StandingsDriver> drivers;   // the game's order
    const StandingsDriver *find(uint32_t driver_stat) const;
};

std::optional<Standings> standings();   // table version 8, the flow thread

} // namespace swofty::career
```

## What it reads

The read walks the season's standings (`F1SeasonStandings`, embedded in `Season` at +0x2a8):

- **Order and totals:** the latest session of the cumulative table (+0x40). The game sorts it after every session: the higher total first, then the countback of finishing positions, then the lower key. The dropped scores rule rewrites its totals, so `points` is the counted total and the order is the game's.
- **A round's points:** the round's points sessions (+0x20) added up. The game copies its extra points list (+0xa8) into one of the current round's points sessions, so a round's points include the extra points.
- **A round's finishes:** the driver's record in each of the round's results sessions (+0xd0), with the position made 1-based.

A record belongs to a driver when its key matches in the kind byte and the driver stat id (the key's high half), as in the game's countback.

`standings()` returns nothing:

- off the flow thread, with no career loaded or while the career is busy;
- when one of the standings' vectors does not read whole;
- on a build the read is not proven on: Steam 1.0.145 and EA App 1.0.145 are proven, with the KB build;
- on a core without the call (`SWOFTY_CAREER_OPS_VERSION` below 8).

A season with nothing raced reads as an empty `drivers` list.

## The C call

```c
int (*standings)(char *json, size_t cap, size_t *len);   /* SwoftyCareerOps, version 8 */
```

```json
{"season": 0, "rounds": 2, "drivers": [
  {"driver": 9, "position": 1, "points": 58, "rounds": [
    {"points": 25, "finishes": [{"session": 12, "position": 2, "status": 0}, {"session": 15, "position": 2, "status": 0}]},
    {"points": 33, "finishes": [{"session": 12, "position": 1, "status": 0}, {"session": 15, "position": 1, "status": 0}]}]}]}
```

It returns `SWOFTY_TOO_SMALL` with `*len` set to the JSON's size when `cap` is too small, `SWOFTY_ASSET_LATE` outside a loaded career and `SWOFTY_ASSET_UNAVAILABLE` on a build the read is not proven on. `career::standings()` grows its buffer and calls it again.
