#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <vector>

namespace runtime::standings_drop {

struct Record {
    uint32_t key;
    int32_t points;
};

struct PointsSession {
    uint32_t id = 0;
    std::vector<Record> records;
};

using PointsRound = std::vector<PointsSession>;

inline bool same_key(uint32_t a, uint32_t b) { return (a & 0xffff00ffu) == (b & 0xffff00ffu); }

inline int64_t best_sum(std::vector<int32_t> values, size_t best)
{
    std::sort(values.begin(), values.end(), std::greater<int32_t>());
    int64_t total = 0;
    for (size_t i = 0; i < values.size() && i < best; i++)
        total += values[i];
    return total;
}

inline int32_t counted_total(const std::vector<PointsRound> &rounds, size_t round, uint32_t session_id, uint32_t key, size_t best)
{
    std::vector<int32_t> per_round;
    for (size_t j = 0; j <= round && j < rounds.size(); j++) {
        int64_t sum = 0;
        for (const PointsSession &s : rounds[j]) {
            for (const Record &r : s.records)
                if (same_key(r.key, key))
                    sum += r.points;
            if (j == round && s.id == session_id)
                break;
        }
        per_round.push_back(static_cast<int32_t>(sum));
    }
    return static_cast<int32_t>(best_sum(per_round, best));
}

using Read = bool (*)(uint64_t address, void *out, size_t n, void *user);

// The season standings' two sides (+0x20 the points records, +0x40 the table) are each a vector of 0x20-byte rounds;
// a round holds a vector of 0x28-byte sessions at +0x08..+0x10 with the session id at +0x20, and a points session holds
// its 8-byte records at +0x08..+0x10.
constexpr size_t POINTS_ROUNDS = 0x20, TABLE_ROUNDS = 0x40;
constexpr size_t ROUND_SIZE = 0x20, SESSION_SIZE = 0x28, RECORD_SIZE = 8;
constexpr size_t SESSION_ID = 0x20;
constexpr size_t MAX_ROUNDS = 64, MAX_SESSIONS = 64, MAX_RECORDS = 128;

template <class T> bool read_as(Read read, void *user, uint64_t at, T &out) { return read(at, &out, sizeof out, user); }

struct RoundView {
    uint64_t sessions_begin = 0, sessions_end = 0;
};

// The number of rounds in the round vector at `array_field`, at most MAX_ROUNDS; 0 when its bytes are not a whole
// number of rounds.
inline size_t round_count(uint64_t array_field, Read read, void *user)
{
    uint64_t b = 0, e = 0;
    if (!read_as(read, user, array_field, b) || !read_as(read, user, array_field + 8, e) || e < b || (e - b) % ROUND_SIZE)
        return 0;
    return std::min<size_t>((e - b) / ROUND_SIZE, MAX_ROUNDS);
}

// Round `index` of the round vector at `array_field`: false when it is past the vector's end, or when the vector or
// the round's sessions are not a whole number of entries, or the round has more than MAX_SESSIONS sessions.
inline bool round_at(uint64_t array_field, size_t index, Read read, void *user, RoundView &out)
{
    uint64_t b = 0, e = 0;
    if (!read_as(read, user, array_field, b) || !read_as(read, user, array_field + 8, e) || e < b || (e - b) % ROUND_SIZE ||
        index >= (e - b) / ROUND_SIZE)
        return false;
    uint64_t r = b + index * ROUND_SIZE;
    return read_as(read, user, r + 8, out.sessions_begin) && read_as(read, user, r + 16, out.sessions_end) &&
           out.sessions_end >= out.sessions_begin && (out.sessions_end - out.sessions_begin) % SESSION_SIZE == 0 &&
           (out.sessions_end - out.sessions_begin) / SESSION_SIZE <= MAX_SESSIONS;
}

// Every round's sessions and point records from the points side of the standings at `standings`; false, with `out`
// partly filled, when any of its vectors or records can't be read whole.
inline bool read_points(uint64_t standings, Read read, void *user, std::vector<PointsRound> &out)
{
    size_t n = round_count(standings + POINTS_ROUNDS, read, user);
    for (size_t r = 0; r < n; r++) {
        RoundView v;
        if (!round_at(standings + POINTS_ROUNDS, r, read, user, v))
            return false;
        PointsRound round;
        for (uint64_t s = v.sessions_begin; s < v.sessions_end; s += SESSION_SIZE) {
            PointsSession ps;
            uint64_t rb = 0, re = 0;
            uint32_t id = 0;
            if (!read_as(read, user, s + 8, rb) || !read_as(read, user, s + 16, re) || re < rb || (re - rb) % RECORD_SIZE ||
                (re - rb) / RECORD_SIZE > MAX_RECORDS || !read_as(read, user, s + SESSION_ID, id))
                return false;
            ps.id = id;
            for (uint64_t p = rb; p < re; p += RECORD_SIZE) {
                Record rec{};
                if (!read_as(read, user, p, rec))
                    return false;
                ps.records.push_back(rec);
            }
            round.push_back(std::move(ps));
        }
        out.push_back(std::move(round));
    }
    return true;
}

// The table's sort key for the game's comparison: the id of the last session of the table's last round that has one,
// in the high half, and that round's index in the low half. False when no round has a session whose id reads.
inline bool table_key(uint64_t standings, Read read, void *user, uint64_t &packed)
{
    for (size_t r = round_count(standings + TABLE_ROUNDS, read, user); r-- > 0;) {
        RoundView v;
        uint32_t id = 0;
        if (!round_at(standings + TABLE_ROUNDS, r, read, user, v) || v.sessions_end == v.sessions_begin)
            continue;
        if (read_as(read, user, v.sessions_end - SESSION_SIZE + SESSION_ID, id)) {
            packed = (uint64_t(id) << 32) | uint32_t(r);
            return true;
        }
    }
    return false;
}

// The career entity's championship (+0x1058) lists its seasons in 0x10-byte entries at +0x548..+0x550, the season at
// entry +8; a season's standings are embedded at Season +0x2a8.
constexpr size_t CHAMPIONSHIP = 0x1058, SEASONS_BEGIN = 0x548, SEASONS_END = 0x550, SEASON_ENTRY = 0x10, ENTRY_SEASON = 8;
constexpr size_t SEASON_STANDINGS = 0x2a8;
constexpr size_t MAX_SEASONS = 4096;

// Whether `standings` are the standings of one of the seasons of the championship of the career entity at `entity`.
// False when the championship or its season vector doesn't read, the vector isn't whole entries or holds more than
// MAX_SEASONS.
inline bool in_championship(uint64_t entity, uint64_t standings, Read read, void *user)
{
    uint64_t champ = 0, b = 0, e = 0;
    if (!read_as(read, user, entity + CHAMPIONSHIP, champ) || !champ || !read_as(read, user, champ + SEASONS_BEGIN, b) ||
        !read_as(read, user, champ + SEASONS_END, e) || e < b || (e - b) % SEASON_ENTRY || (e - b) / SEASON_ENTRY > MAX_SEASONS)
        return false;
    for (uint64_t p = b; p < e; p += SEASON_ENTRY) {
        uint64_t season = 0;
        if (read_as(read, user, p + ENTRY_SEASON, season) && season && season + SEASON_STANDINGS == standings)
            return true;
    }
    return false;
}

// An entry of a table session: the driver's key and cumulative total.
struct TableEntry {
    uint32_t key;
    int32_t total;
};
static_assert(sizeof(TableEntry) == RECORD_SIZE);

// A table session: its entries at +0x08..+0x10 and its session id at +0x20.
struct TableSession {
    uint64_t entries = 0;
    size_t count = 0;
    uint32_t id = 0;
};

// The table session at `session`; false when its entries are not a whole number of entries, more than MAX_RECORDS,
// or its vector or id doesn't read.
inline bool table_session(uint64_t session, Read read, void *user, TableSession &out)
{
    uint64_t b = 0, e = 0;
    if (!read_as(read, user, session + 8, b) || !read_as(read, user, session + 16, e) || e < b || (e - b) % sizeof(TableEntry) ||
        (e - b) / sizeof(TableEntry) > MAX_RECORDS || !read_as(read, user, session + SESSION_ID, out.id))
        return false;
    out.entries = b;
    out.count = (e - b) / sizeof(TableEntry);
    return true;
}

// The order a table session is sorted in after its totals change (the game's own comparison); `user` is passed on.
using Less = bool (*)(const TableEntry &a, const TableEntry &b, void *user);

// Sets each of the `count` entries' total to its driver's counted total at session `session_id` of round `round`
// (counted_total) and, with `less`, sorts them with it. Returns how many totals changed.
inline size_t recount_session(TableEntry *entries, size_t count, const std::vector<PointsRound> &points, size_t round,
                              uint32_t session_id, size_t best, Less less, void *less_user)
{
    size_t changed = 0;
    for (size_t i = 0; i < count; i++) {
        int32_t counted = counted_total(points, round, session_id, entries[i].key, best);
        changed += counted != entries[i].total;
        entries[i].total = counted;
    }
    if (less)
        std::sort(entries, entries + count, [&](const TableEntry &a, const TableEntry &b) { return less(a, b, less_user); });
    return changed;
}

// Where the `count` entries at `address` can be written in place; null when they can't.
using Entries = TableEntry *(*)(uint64_t address, size_t count, void *user);

struct Recounted {
    size_t sessions = 0, changed = 0;
};

// Recounts every table session of the standings at `standings` (recount_session). A round or session that doesn't
// read whole, or whose entries `entries` can't place, is passed over and not counted in `sessions`.
inline Recounted recount_table(uint64_t standings, const std::vector<PointsRound> &points, size_t best, Read read, void *user,
                               Entries entries, Less less, void *less_user)
{
    Recounted out;
    size_t rounds = round_count(standings + TABLE_ROUNDS, read, user);
    for (size_t r = 0; r < rounds; r++) {
        RoundView v;
        if (!round_at(standings + TABLE_ROUNDS, r, read, user, v))
            continue;
        for (uint64_t s = v.sessions_begin; s < v.sessions_end; s += SESSION_SIZE) {
            TableSession ts;
            TableEntry *at = nullptr;
            if (!table_session(s, read, user, ts) || !(at = entries(ts.entries, ts.count, user)))
                continue;
            out.changed += recount_session(at, ts.count, points, r, ts.id, best, less, less_user);
            out.sessions++;
        }
    }
    return out;
}

}
