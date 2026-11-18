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

inline bool same_key(uint32_t a, uint32_t b)
{
    return (a & 0xffff00ffu) == (b & 0xffff00ffu);
}

inline int64_t best_sum(std::vector<int32_t> values, size_t best)
{
    std::sort(values.begin(), values.end(), std::greater<int32_t>());
    int64_t total = 0;
    for (size_t i = 0; i < values.size() && i < best; i++)
        total += values[i];
    return total;
}

inline int32_t counted_total(const std::vector<PointsRound> &rounds, size_t round, uint32_t session_id, uint32_t key,
                             size_t best)
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

}
