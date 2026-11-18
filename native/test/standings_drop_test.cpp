#include "core/guards/career/standings_drop_logic.hpp"

#include <cstdio>
#include <cstring>
#include <vector>

static int failures;

#define CHECK(cond, msg)                                                                                                \
    do {                                                                                                                \
        if (cond) {                                                                                                     \
            printf("ok  : %s\n", msg);                                                                                  \
        } else {                                                                                                        \
            printf("FAIL: %s\n", msg);                                                                                  \
            failures++;                                                                                                 \
        }                                                                                                               \
    } while (0)

using namespace runtime::standings_drop;

static std::vector<PointsRound> season(const std::vector<int> &per_round, uint32_t key)
{
    std::vector<PointsRound> rounds;
    for (int p : per_round) {
        PointsSession practice{3, {{key, 0}}}, race{15, {{key, p}}};
        rounds.push_back({practice, race});
    }
    return rounds;
}

struct Fake {
    static constexpr uint64_t BASE = 0x100000;
    std::vector<uint8_t> bytes = std::vector<uint8_t>(0x4000);
    uint64_t next = BASE + 0x100;

    uint64_t alloc(size_t n)
    {
        uint64_t at = next;
        next += (n + 15) & ~size_t(15);
        return at;
    }
    void put64(uint64_t at, uint64_t v) { memcpy(&bytes[at - BASE], &v, sizeof v); }
    void put32(uint64_t at, uint32_t v) { memcpy(&bytes[at - BASE], &v, sizeof v); }
};

static bool fake_read(uint64_t at, void *out, size_t n, void *user)
{
    auto *f = static_cast<Fake *>(user);
    if (at < Fake::BASE || at + n > Fake::BASE + f->bytes.size())
        return false;
    memcpy(out, &f->bytes[at - Fake::BASE], n);
    return true;
}

// Writes `rounds` as the game lays them out and points the {begin, end} pair at `field` at them.
static void put_rounds(Fake &f, uint64_t field, const std::vector<PointsRound> &rounds)
{
    uint64_t rb = f.alloc(rounds.size() * ROUND_SIZE);
    for (size_t r = 0; r < rounds.size(); r++) {
        const PointsRound &sessions = rounds[r];
        uint64_t sb = f.alloc(sessions.size() * SESSION_SIZE);
        for (size_t s = 0; s < sessions.size(); s++) {
            uint64_t at = sb + s * SESSION_SIZE;
            const std::vector<Record> &records = sessions[s].records;
            uint64_t recs = f.alloc(records.size() * RECORD_SIZE);
            for (size_t i = 0; i < records.size(); i++) {
                f.put32(recs + i * RECORD_SIZE, records[i].key);
                f.put32(recs + i * RECORD_SIZE + 4, uint32_t(records[i].points));
            }
            f.put64(at + 8, recs);
            f.put64(at + 16, recs + records.size() * RECORD_SIZE);
            f.put32(at + SESSION_ID, sessions[s].id);
        }
        f.put64(rb + r * ROUND_SIZE + 8, sb);
        f.put64(rb + r * ROUND_SIZE + 16, sb + sessions.size() * SESSION_SIZE);
    }
    f.put64(field, rb);
    f.put64(field + 8, rb + rounds.size() * ROUND_SIZE);
}

static uint64_t session_at(Fake &f, uint64_t field, size_t round, size_t session)
{
    uint64_t rb = 0, sb = 0;
    fake_read(field, &rb, sizeof rb, &f);
    fake_read(rb + round * ROUND_SIZE + 8, &sb, sizeof sb, &f);
    return sb + session * SESSION_SIZE;
}

static void walks()
{
    const uint64_t standings = Fake::BASE;
    const uint32_t key = 0x00050001, other = 0x00060001;
    const std::vector<PointsRound> points = {
        {{3, {{key, 0}}}, {15, {{key, 25}, {other, 18}}}},
        {{15, {{key, 18}, {other, 25}}}},
    };
    {
        Fake f;
        put_rounds(f, standings + POINTS_ROUNDS, points);
        std::vector<PointsRound> read;
        CHECK(read_points(standings, fake_read, &f, read), "a whole points side reads");
        CHECK(read.size() == 2 && read[0].size() == 2 && read[1].size() == 1, "every round and session is read");
        CHECK(read[0][0].id == 3 && read[0][1].id == 15 && read[1][0].id == 15, "each session keeps its id");
        CHECK(read[0][1].records.size() == 2 && read[0][1].records[1].key == other && read[0][1].records[1].points == 18,
              "each record keeps its key and points");
        CHECK(counted_total(read, 1, 15, key, 1) == 25 && counted_total(read, 1, 15, other, 1) == 25,
              "the read rounds count each driver's best result");
    }
    {
        Fake f;
        std::vector<PointsRound> read;
        CHECK(read_points(standings, fake_read, &f, read) && read.empty(), "a season with no rounds reads as none");
        CHECK(read_points(0x10, fake_read, &f, read) && read.empty(), "standings that can't be read have no rounds to recount");
    }
    {
        Fake f;
        f.put64(standings + TABLE_ROUNDS, 0x101000);
        f.put64(standings + TABLE_ROUNDS + 8, 0x101000 + 70 * ROUND_SIZE);
        CHECK(round_count(standings + TABLE_ROUNDS, fake_read, &f) == MAX_ROUNDS, "a round count past 64 is held at 64");
        f.put64(standings + TABLE_ROUNDS + 8, 0x101000 + ROUND_SIZE + 8);
        CHECK(round_count(standings + TABLE_ROUNDS, fake_read, &f) == 0, "a round vector that isn't whole rounds has none");
        f.put64(standings + TABLE_ROUNDS + 8, 0x100ff0);
        CHECK(round_count(standings + TABLE_ROUNDS, fake_read, &f) == 0, "a round vector that ends before it begins has none");
    }
    {
        Fake f;
        put_rounds(f, standings + POINTS_ROUNDS, points);
        RoundView v;
        CHECK(round_at(standings + POINTS_ROUNDS, 1, fake_read, &f, v) && v.sessions_end - v.sessions_begin == SESSION_SIZE,
              "the last round is found with its one session");
        CHECK(!round_at(standings + POINTS_ROUNDS, 2, fake_read, &f, v), "a round past the vector's end is not read");
        uint64_t rb = 0;
        fake_read(standings + POINTS_ROUNDS, &rb, sizeof rb, &f);
        f.put64(rb + 16, v.sessions_begin + SESSION_SIZE + 8);
        f.put64(rb + 8, v.sessions_begin);
        CHECK(!round_at(standings + POINTS_ROUNDS, 0, fake_read, &f, v), "a session vector that isn't whole sessions is refused");
        f.put64(rb + 16, v.sessions_begin + (MAX_SESSIONS + 1) * SESSION_SIZE);
        CHECK(!round_at(standings + POINTS_ROUNDS, 0, fake_read, &f, v), "a round with more than 64 sessions is refused");
        std::vector<PointsRound> read;
        CHECK(!read_points(standings, fake_read, &f, read), "a refused round fails the whole points side");
    }
    {
        Fake f;
        put_rounds(f, standings + POINTS_ROUNDS, points);
        uint64_t race = session_at(f, standings + POINTS_ROUNDS, 0, 1), recs = 0;
        fake_read(race + 8, &recs, sizeof recs, &f);
        std::vector<PointsRound> read;
        f.put64(race + 16, recs + 12);
        CHECK(!read_points(standings, fake_read, &f, read), "a record vector that isn't whole records fails the points side");
        f.put64(race + 16, recs + (MAX_RECORDS + 1) * RECORD_SIZE);
        read.clear();
        CHECK(!read_points(standings, fake_read, &f, read), "a session with more than 128 records fails the points side");
        f.put64(race + 8, 0x900000);
        f.put64(race + 16, 0x900000 + RECORD_SIZE);
        read.clear();
        CHECK(!read_points(standings, fake_read, &f, read), "a record that can't be read fails the points side");
    }
    {
        Fake f;
        put_rounds(f, standings + TABLE_ROUNDS, {{{3, {}}, {15, {}}}, {{3, {}}}, {}});
        uint64_t packed = 0;
        CHECK(table_key(standings, fake_read, &f, packed) && packed == ((uint64_t(3) << 32) | 1),
              "the key is the last session of the last round that has one, with that round's index");
        uint64_t rb = 0;
        fake_read(standings + TABLE_ROUNDS, &rb, sizeof rb, &f);
        uint64_t sb = 0;
        fake_read(rb + ROUND_SIZE + 8, &sb, sizeof sb, &f);
        f.put64(rb + ROUND_SIZE + 16, sb + SESSION_SIZE + 4);
        CHECK(table_key(standings, fake_read, &f, packed) && packed == ((uint64_t(15) << 32) | 0),
              "a malformed round is passed over for the one before it");
    }
    {
        Fake f;
        put_rounds(f, standings + TABLE_ROUNDS, {{}, {}});
        uint64_t packed = 7;
        CHECK(!table_key(standings, fake_read, &f, packed) && packed == 7, "a table with no sessions has no key");
    }
}

int main()
{
    CHECK(best_sum({1, 9, 4}, 2) == 13, "best_sum keeps the two best of three");
    CHECK(best_sum({1, 9, 4}, 11) == 14, "best_sum with fewer values than the limit sums them all");
    CHECK(best_sum({}, 11) == 0, "best_sum of nothing is 0");

    const uint32_t key = 0x00050001;
    std::vector<int> pts(16, 0);
    for (int i = 0; i < 16; i++)
        pts[i] = (i % 3 == 0) ? 9 : (i % 3 == 1 ? 6 : 0);
    auto rounds = season(pts, key);
    CHECK(counted_total(rounds, 10, 15, key, 11) == 9 * 4 + 6 * 4 + 0 * 3 + 0, "all of the first 11 rounds count");
    int64_t gross = 0;
    for (int p : pts)
        gross += p;
    int32_t counted = counted_total(rounds, 15, 15, key, 11);
    CHECK(gross == 9 * 6 + 6 * 5 && counted <= gross, "the 16 round total never exceeds the gross total");
    CHECK(counted == 9 * 6 + 6 * 5, "the five dropped results are the zeros, so no points are lost");

    std::vector<int> all_six(16, 6);
    int64_t six_gross = 0;
    for (int p : all_six)
        six_gross += p;
    CHECK(six_gross == 96 && counted_total(season(all_six, key), 15, 15, key, 11) < six_gross, "sixteen sixes drop five");
    CHECK(counted_total(season(all_six, key), 15, 15, key, 11) == 66, "sixteen sixes count as eleven");
    CHECK(counted_total(season(all_six, key), 11, 15, key, 11) == 66, "twelve sixes count as eleven");
    CHECK(counted_total(season(all_six, key), 10, 15, key, 11) == 66, "eleven sixes count all");
    CHECK(counted_total(season(all_six, key), 9, 15, key, 11) == 60, "ten sixes count all");

    std::vector<int> late(16, 0);
    late[0] = 1;
    late[1] = 1;
    late[2] = 1;
    late[3] = 1;
    late[4] = 1;
    late[5] = 1;
    for (int i = 6; i < 16; i++)
        late[i] = 9;
    CHECK(counted_total(season(late, key), 15, 15, key, 11) == 90 + 1, "the ten wins and the best of the six ones count");

    auto cut = season(all_six, key);
    CHECK(counted_total(cut, 0, 3, key, 11) == 0, "a session cutoff before the race leaves the round at 0");
    CHECK(counted_total(cut, 0, 15, key, 11) == 6, "the cutoff at the race includes it");

    CHECK(counted_total(rounds, 15, 15, 0x00990002, 11) == 0, "a driver with no records has 0");
    CHECK(same_key(0x00050001, 0x00050201), "the game's key match ignores bits 8 to 15");
    CHECK(!same_key(0x00050001, 0x00060001), "the key match compares the high half");
    walks();
    return failures ? 1 : 0;
}
