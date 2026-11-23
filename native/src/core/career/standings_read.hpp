#pragma once

#include "core/guards/career/standings_drop_logic.hpp"

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

// The drivers' standings of a season as the game holds them (F1SeasonStandings, embedded in Season at +0x2a8; see
// core/guards/career/standings_drop.cpp for the layout): the order and each driver's total come from the cumulative
// table's latest session (+0x40), which the game's recompute sorts with Career_Compare_StandingsEntries (total, the
// countback over each round's counted session, then the key) and the dropped scores rule rewrites. Each round's points
// add up that round's points sessions (+0x20). The standings' vt+0x40 (KB 0x14961b930), which the points recompute
// calls first, copies the extra points list (+0xa8) into one of the current round's points sessions, so the points
// side already holds the extra points. Each round's finishes are the driver's result in each of the round's results
// sessions (+0xd0: rounds of 0x20, sessions of 0x28, records {u32 key, i32 position, i32 fastest lap rank, u32
// status} of 0x10 bytes). A record belongs to a driver when its key matches the table's in the kind byte and the
// driver stat id (the high half), as in the game's countback.
namespace career::standings {

namespace sd = runtime::standings_drop;

constexpr size_t RESULTS_ROUNDS = 0xd0, RESULT_SIZE = 0x10;
constexpr size_t RESULT_POSITION = 4, RESULT_STATUS = 0xc;

struct Finish {
    uint32_t session = 0;
    int32_t position = 0;
    uint32_t status = 0;
};

struct Round {
    int32_t points = 0;
    std::vector<Finish> finishes;
};

struct Driver {
    uint32_t key = 0;
    int32_t points = 0;
    std::vector<Round> rounds;
};

struct Standings {
    size_t rounds = 0;
    std::vector<Driver> drivers;
    std::vector<std::vector<uint32_t>> sessions;
};

// The constructors' side of a season's standings is the same model at Season +0x3b8: the points per round and session
// (+0x20, records {u32 team key, i32 points}) and the sorted cumulative table (+0x40). Until a session is scored its
// table is empty and the game shows the season-start order at +0xd8..+0xe0 ({u32 team key, u32} elements, filled by
// 0x1413cf8a0 from the rebuild's team order).
constexpr size_t SEASON_CONSTRUCTORS = 0x3b8, START_ORDER = 0xd8, START_ORDER_SIZE = 8;

struct Team {
    uint32_t key = 0;
    int32_t points = 0;
    std::vector<int32_t> rounds;
};

struct Constructors {
    size_t rounds = 0;
    std::vector<Team> teams;
};

struct ResultRecord {
    uint32_t key = 0;
    int32_t position = 0;
    uint32_t status = 0;
};

struct ResultsSession {
    uint32_t id = 0;
    std::vector<ResultRecord> records;
};

using ResultsRound = std::vector<ResultsSession>;

// The {begin, end} round vector at `field`: false when it doesn't read, isn't whole rounds or holds more than
// MAX_ROUNDS.
inline bool rounds_of(uint64_t field, sd::Read read, void *user, size_t &count)
{
    uint64_t b = 0, e = 0;
    if (!sd::read_as(read, user, field, b) || !sd::read_as(read, user, field + 8, e) || e < b || (e - b) % sd::ROUND_SIZE ||
        (e - b) / sd::ROUND_SIZE > sd::MAX_ROUNDS)
        return false;
    count = (e - b) / sd::ROUND_SIZE;
    return true;
}

// A session's {begin, end} records at +0x08: false when they don't read, aren't whole records of `size` bytes or
// number more than MAX_RECORDS.
inline bool records_of(uint64_t session, size_t size, sd::Read read, void *user, uint64_t &begin, size_t &count)
{
    uint64_t e = 0;
    if (!sd::read_as(read, user, session + 8, begin) || !sd::read_as(read, user, session + 16, e) || e < begin ||
        (e - begin) % size || (e - begin) / size > sd::MAX_RECORDS)
        return false;
    count = (e - begin) / size;
    return true;
}

// The table's latest session: the last session of the last round that has one. Empty when no round has a session.
inline bool read_order(uint64_t standings, sd::Read read, void *user, std::vector<sd::Record> &out)
{
    size_t n = 0;
    if (!rounds_of(standings + sd::TABLE_ROUNDS, read, user, n))
        return false;
    for (size_t r = n; r-- > 0;) {
        sd::RoundView v;
        if (!sd::round_at(standings + sd::TABLE_ROUNDS, r, read, user, v))
            return false;
        if (v.sessions_end == v.sessions_begin)
            continue;
        uint64_t rb = 0;
        size_t count = 0;
        if (!records_of(v.sessions_end - sd::SESSION_SIZE, sd::RECORD_SIZE, read, user, rb, count))
            return false;
        for (size_t i = 0; i < count; i++) {
            sd::Record rec{};
            if (!sd::read_as(read, user, rb + i * sd::RECORD_SIZE, rec))
                return false;
            out.push_back(rec);
        }
        return true;
    }
    return true;
}

inline bool read_results(uint64_t standings, sd::Read read, void *user, std::vector<ResultsRound> &out)
{
    size_t n = 0;
    if (!rounds_of(standings + RESULTS_ROUNDS, read, user, n))
        return false;
    for (size_t r = 0; r < n; r++) {
        sd::RoundView v;
        if (!sd::round_at(standings + RESULTS_ROUNDS, r, read, user, v))
            return false;
        ResultsRound round;
        for (uint64_t s = v.sessions_begin; s < v.sessions_end; s += sd::SESSION_SIZE) {
            ResultsSession rs;
            uint64_t rb = 0;
            size_t count = 0;
            if (!records_of(s, RESULT_SIZE, read, user, rb, count) || !sd::read_as(read, user, s + sd::SESSION_ID, rs.id))
                return false;
            for (size_t i = 0; i < count; i++) {
                uint64_t p = rb + i * RESULT_SIZE;
                ResultRecord rec;
                if (!sd::read_as(read, user, p, rec.key) || !sd::read_as(read, user, p + RESULT_POSITION, rec.position) ||
                    !sd::read_as(read, user, p + RESULT_STATUS, rec.status))
                    return false;
                rs.records.push_back(rec);
            }
            round.push_back(std::move(rs));
        }
        out.push_back(std::move(round));
    }
    return true;
}

// The season's standings at `standings`: false when any vector, session or record the read walks doesn't read whole.
inline bool read(uint64_t standings, sd::Read read, void *user, Standings &out)
{
    std::vector<sd::Record> order;
    std::vector<sd::PointsRound> points;
    std::vector<ResultsRound> results;
    size_t n = 0;
    if (!read_order(standings, read, user, order) || !rounds_of(standings + sd::POINTS_ROUNDS, read, user, n) ||
        !sd::read_points(standings, read, user, points) || !read_results(standings, read, user, results))
        return false;
    out = {};
    out.rounds = std::max(points.size(), results.size());
    out.sessions.resize(out.rounds);
    for (size_t r = 0; r < results.size(); r++)
        for (const ResultsSession &s : results[r])
            out.sessions[r].push_back(s.id);
    for (const sd::Record &entry : order) {
        Driver d;
        d.key = entry.key;
        d.points = entry.points;
        d.rounds.resize(out.rounds);
        for (size_t r = 0; r < points.size(); r++)
            for (const sd::PointsSession &s : points[r])
                for (const sd::Record &rec : s.records)
                    if (sd::same_key(rec.key, entry.key))
                        d.rounds[r].points += rec.points;
        for (size_t r = 0; r < results.size(); r++)
            for (const ResultsSession &s : results[r])
                for (const ResultRecord &rec : s.records)
                    if (sd::same_key(rec.key, entry.key)) {
                        d.rounds[r].finishes.push_back({s.id, rec.position + 1, rec.status});
                        break;
                    }
        out.drivers.push_back(std::move(d));
    }
    return true;
}

inline bool read_start_order(uint64_t side, sd::Read read, void *user, std::vector<sd::Record> &out)
{
    uint64_t b = 0, e = 0;
    if (!sd::read_as(read, user, side + START_ORDER, b) || !sd::read_as(read, user, side + START_ORDER + 8, e) || e < b ||
        (e - b) % START_ORDER_SIZE || (e - b) / START_ORDER_SIZE > sd::MAX_RECORDS)
        return false;
    for (uint64_t p = b; p < e; p += START_ORDER_SIZE) {
        uint32_t key = 0;
        if (!sd::read_as(read, user, p, key))
            return false;
        out.push_back({key, 0});
    }
    return true;
}

// The constructors' standings at `side` (Season + SEASON_CONSTRUCTORS): the table's latest session gives the order and
// the totals, the season-start order while nothing is scored; each round's points add up its points sessions. False
// when a vector, session or record the read walks doesn't read whole.
inline bool read_constructors(uint64_t side, sd::Read read, void *user, Constructors &out)
{
    std::vector<sd::Record> order;
    std::vector<sd::PointsRound> points;
    if (!read_order(side, read, user, order) || !sd::read_points(side, read, user, points))
        return false;
    if (order.empty() && !read_start_order(side, read, user, order))
        return false;
    out = {};
    out.rounds = points.size();
    for (const sd::Record &entry : order) {
        Team t;
        t.key = entry.key;
        t.points = entry.points;
        t.rounds.resize(out.rounds);
        for (size_t r = 0; r < points.size(); r++)
            for (const sd::PointsSession &s : points[r])
                for (const sd::Record &rec : s.records)
                    if (sd::same_key(rec.key, entry.key))
                        t.rounds[r] += rec.points;
        out.teams.push_back(std::move(t));
    }
    return true;
}

inline std::string json(const Standings &s, uint32_t season_index)
{
    std::string o = "{\"season\":" + std::to_string(season_index) + ",\"rounds\":" + std::to_string(s.rounds) + ",\"drivers\":[";
    for (size_t i = 0; i < s.drivers.size(); i++) {
        const Driver &d = s.drivers[i];
        o += i ? ",{" : "{";
        o += "\"driver\":" + std::to_string(d.key >> 16) + ",\"position\":" + std::to_string(i + 1) +
             ",\"points\":" + std::to_string(d.points) + ",\"rounds\":[";
        for (size_t r = 0; r < d.rounds.size(); r++) {
            o += r ? ",{" : "{";
            o += "\"points\":" + std::to_string(d.rounds[r].points) + ",\"finishes\":[";
            for (size_t f = 0; f < d.rounds[r].finishes.size(); f++) {
                const Finish &x = d.rounds[r].finishes[f];
                o += f ? ",{" : "{";
                o += "\"session\":" + std::to_string(x.session) + ",\"position\":" + std::to_string(x.position) +
                     ",\"status\":" + std::to_string(x.status) + "}";
            }
            o += "]}";
        }
        o += "]}";
    }
    return o + "]}";
}

}
