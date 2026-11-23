#pragma once

#include "core/career/standings_read.hpp"

#include "swofty_api.h"

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <vector>

// The championship standings snapshot the core serves through SwoftyCoreStandings: the drivers' and constructors'
// tables (standings_read.hpp) joined with who raced for which team. A driver row's team and number are his career
// row's while he has a seat, else his last round's. The wins and podiums count main-race finishes; a round's main
// race is the last race session (ids 15 to 17) its weekend lists, or its last results session when the weekend's
// sessions don't read. A constructor's wins and podiums are those of the drivers it fielded in that round: the
// weekend's own line-up copy, else the driver row's team.
namespace career::standings {

constexpr uint32_t NONE = SWOFTY_STANDINGS_NONE;
constexpr uint32_t RACE_FIRST = 15, RACE_LAST = 17;

struct Seat {
    uint32_t driver = NONE;
    uint32_t team = NONE;
    uint8_t number = 0;
};

struct Lineups {
    std::vector<Seat> now;
    std::vector<std::vector<Seat>> rounds;
    std::vector<uint32_t> races;
};

struct Where {
    uint32_t mode = 0;
    int32_t year = 0, season_index = 0, round = 0, rounds = 0;
    uint32_t player_driver = NONE, player_team = NONE;
};

struct Snapshot {
    SwoftyStandingsSeason season{};
    std::vector<SwoftyStandingsRow> drivers, constructors;
};

inline uint32_t main_race(const std::vector<uint32_t> &weekend_sessions)
{
    uint32_t race = 0;
    for (uint32_t id : weekend_sessions)
        if (id >= RACE_FIRST && id <= RACE_LAST)
            race = id;
    return race;
}

// The id of round r's main race among its results sessions; 0 while it has none. Results that name no race session
// at all (ids outside 15 to 17) take the round's last results session.
inline uint32_t race_of(const Standings &s, const Lineups &l, size_t r)
{
    if (r >= s.sessions.size() || s.sessions[r].empty())
        return 0;
    const std::vector<uint32_t> &ids = s.sessions[r];
    uint32_t want = r < l.races.size() ? l.races[r] : 0;
    if (!want || !main_race(ids))
        return ids.back();
    return std::find(ids.begin(), ids.end(), want) != ids.end() ? want : 0;
}

inline const Seat *seat_of(const std::vector<Seat> &seats, uint32_t driver)
{
    for (const Seat &s : seats)
        if (s.driver == driver)
            return &s;
    return nullptr;
}

inline Seat seat_now(const Lineups &l, uint32_t driver)
{
    if (const Seat *s = seat_of(l.now, driver); s && s->team != NONE)
        return *s;
    for (size_t r = l.rounds.size(); r-- > 0;)
        if (const Seat *s = seat_of(l.rounds[r], driver); s && s->team != NONE)
            return *s;
    const Seat *s = seat_of(l.now, driver);
    return s ? *s : Seat{driver, NONE, 0};
}

inline uint32_t team_in_round(const Lineups &l, size_t r, uint32_t driver, uint32_t fallback)
{
    const Seat *s = r < l.rounds.size() ? seat_of(l.rounds[r], driver) : nullptr;
    return s && s->team != NONE ? s->team : fallback;
}

inline SwoftyStandingsRow blank_row()
{
    SwoftyStandingsRow row;
    std::memset(&row, 0, sizeof row);
    row.driver = row.team = NONE;
    return row;
}

inline void fill_rounds(SwoftyStandingsRow &row, const std::vector<int32_t> &points)
{
    for (size_t r = 0; r < points.size() && r < SWOFTY_STANDINGS_MAX_ROUNDS; r++)
        row.round_points[r] = points[r];
}

inline Snapshot snapshot(const Standings &d, const Constructors &c, const Lineups &l, const Where &w)
{
    Snapshot out;
    std::vector<uint32_t> races(d.rounds);
    int32_t raced = 0;
    for (size_t r = 0; r < d.rounds; r++)
        raced += (races[r] = race_of(d, l, r)) != 0;

    struct Credit {
        uint32_t team;
        bool win;
    };
    std::vector<Credit> credits;
    for (size_t i = 0; i < d.drivers.size(); i++) {
        const Driver &dr = d.drivers[i];
        uint32_t stat = dr.key >> 16;
        Seat seat = seat_now(l, stat);
        SwoftyStandingsRow row = blank_row();
        row.driver = stat;
        row.team = seat.team;
        row.race_number = seat.number;
        row.position = static_cast<int32_t>(i + 1);
        row.points = dr.points;
        row.flags = stat == w.player_driver ? SWOFTY_STANDINGS_PLAYER : 0;
        std::vector<int32_t> points;
        for (size_t r = 0; r < dr.rounds.size(); r++) {
            points.push_back(dr.rounds[r].points);
            for (const Finish &f : dr.rounds[r].finishes) {
                if (!races[r] || f.session != races[r] || f.status >= 2 || f.position < 1 || f.position > 3)
                    continue;
                row.podiums++;
                row.wins += f.position == 1;
                credits.push_back({team_in_round(l, r, stat, seat.team), f.position == 1});
            }
        }
        fill_rounds(row, points);
        out.drivers.push_back(row);
    }

    for (size_t i = 0; i < c.teams.size(); i++) {
        const Team &t = c.teams[i];
        SwoftyStandingsRow row = blank_row();
        row.team = t.key >> 16;
        row.position = static_cast<int32_t>(i + 1);
        row.points = t.points;
        row.flags = row.team == w.player_team ? SWOFTY_STANDINGS_PLAYER : 0;
        for (const Credit &k : credits)
            if (k.team == row.team) {
                row.podiums++;
                row.wins += k.win;
            }
        fill_rounds(row, t.rounds);
        out.constructors.push_back(row);
    }

    SwoftyStandingsSeason &s = out.season;
    s.mode = w.mode;
    s.year = w.year;
    s.season_index = w.season_index;
    s.round = w.round;
    s.rounds = w.rounds;
    s.scored = static_cast<int32_t>(std::max(d.rounds, c.rounds));
    s.raced = raced;
    s.flags = w.rounds > 0 && static_cast<size_t>(w.rounds) <= races.size() && races[static_cast<size_t>(w.rounds) - 1]
                  ? SWOFTY_STANDINGS_COMPLETE
                  : 0;
    s.drivers = static_cast<uint32_t>(out.drivers.size());
    s.constructors = static_cast<uint32_t>(out.constructors.size());
    return out;
}

inline bool same(const Snapshot &a, const Snapshot &b)
{
    SwoftyStandingsSeason x = a.season, y = b.season;
    x.seq = y.seq = 0;
    auto rows = [](const std::vector<SwoftyStandingsRow> &p, const std::vector<SwoftyStandingsRow> &q) {
        return p.size() == q.size() && (p.empty() || !std::memcmp(p.data(), q.data(), p.size() * sizeof p[0]));
    };
    return !std::memcmp(&x, &y, sizeof x) && rows(a.drivers, b.drivers) && rows(a.constructors, b.constructors);
}

// Up to cap rows into out and the row count into *count, as SwoftyStandings.drivers and constructors answer.
inline int copy_rows(const std::vector<SwoftyStandingsRow> &rows, SwoftyStandingsRow *out, uint32_t cap, uint32_t *count)
{
    if (!count || (cap && !out))
        return SWOFTY_ASSET_INVALID;
    *count = static_cast<uint32_t>(rows.size());
    for (size_t i = 0; i < rows.size() && i < cap; i++)
        out[i] = rows[i];
    return SWOFTY_ASSET_OK;
}

}
