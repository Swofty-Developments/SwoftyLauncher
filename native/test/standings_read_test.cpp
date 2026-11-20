#include "core/career/standings_read.hpp"

#include <cstdio>
#include <cstring>
#include <string>
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
namespace sd = runtime::standings_drop;

struct Fake {
    static constexpr uint64_t BASE = 0x100000;
    std::vector<uint8_t> bytes = std::vector<uint8_t>(0x10000);
    uint64_t next = BASE + 0x200;
    uint64_t standings = BASE;

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

struct Rec {
    uint32_t key = 0;
    int32_t a = 0, b = 0;
    uint32_t c = 0;
};

struct FakeSession {
    uint32_t id;
    std::vector<Rec> records;
};

using FakeRound = std::vector<FakeSession>;

// Lays `rounds` out as the game does at the {begin, end} pair `field`: 0x20-byte rounds, 0x28-byte sessions with
// the id at +0x20, and records of `record_size` bytes (8 for points and the table, 0x10 for results).
static void put_rounds(Fake &f, uint64_t field, const std::vector<FakeRound> &rounds, size_t record_size)
{
    uint64_t rb = f.alloc(rounds.size() * sd::ROUND_SIZE);
    for (size_t r = 0; r < rounds.size(); r++) {
        uint64_t sb = f.alloc(rounds[r].size() * sd::SESSION_SIZE);
        for (size_t s = 0; s < rounds[r].size(); s++) {
            const FakeSession &ses = rounds[r][s];
            uint64_t at = sb + s * sd::SESSION_SIZE;
            uint64_t recs = f.alloc(ses.records.size() * record_size);
            for (size_t i = 0; i < ses.records.size(); i++) {
                const Rec &x = ses.records[i];
                uint64_t p = recs + i * record_size;
                f.put32(p, x.key);
                f.put32(p + 4, uint32_t(x.a));
                if (record_size == RESULT_SIZE) {
                    f.put32(p + 8, uint32_t(x.b));
                    f.put32(p + 12, x.c);
                }
            }
            f.put64(at + 8, recs);
            f.put64(at + 16, recs + ses.records.size() * record_size);
            f.put32(at + sd::SESSION_ID, ses.id);
        }
        f.put64(rb + r * sd::ROUND_SIZE + 8, sb);
        f.put64(rb + r * sd::ROUND_SIZE + 16, sb + rounds[r].size() * sd::SESSION_SIZE);
    }
    f.put64(field, rb);
    f.put64(field + 8, rb + rounds.size() * sd::ROUND_SIZE);
}

static uint32_t key_of(uint16_t stat, uint8_t kind = 1, uint8_t extra = 0)
{
    return uint32_t(stat) << 16 | uint32_t(extra) << 8 | kind;
}

static const Driver *find(const Standings &s, uint32_t stat)
{
    for (const Driver &d : s.drivers)
        if ((d.key >> 16) == stat)
            return &d;
    return nullptr;
}

// Two rounds, each a sprint (id 12) and a race (id 15); three drivers. The table's last session holds the game's
// sorted order, which differs from the points order (the game's countback puts driver 9 ahead of driver 7 on equal
// totals), and a third session in round 2, the one the extra points were copied into, gives driver 5 two points.
static Fake season()
{
    Fake f;
    uint32_t a = key_of(7), b = key_of(9), c = key_of(5);
    std::vector<FakeRound> points = {
        {{12, {{a, 8}, {b, 7}, {c, 6}}}, {15, {{a, 25}, {b, 18}, {c, 15}}}},
        {{12, {{b, 8}, {a, 7}, {c, 6}}}, {15, {{b, 25}, {a, 18}, {c, 15}}}, {0, {{c, 2}}}},
    };
    std::vector<FakeRound> results = {
        {{12, {{a, 0, 1, 0}, {b, 1, 0, 0}, {c, 2, 2, 0}}}, {15, {{a, 0, 0, 0}, {b, 1, 1, 0}, {c, 2, 2, 0}}}},
        {{12, {{b, 0, 0, 0}, {a, 1, 1, 0}, {c, 2, 2, 0}}}, {15, {{b, 0, 0, 0}, {a, 1, 1, 0}, {c, 2, 2, 3}}}},
    };
    std::vector<FakeRound> table = {
        {{12, {{a, 8}, {b, 7}, {c, 6}}}, {15, {{a, 33}, {b, 25}, {c, 21}}}},
        {{12, {{b, 33}, {a, 40}, {c, 27}}}, {15, {{b, 58}, {a, 58}, {c, 42}}}, {0, {{b, 58}, {a, 58}, {c, 44}}}},
    };
    put_rounds(f, f.standings + sd::POINTS_ROUNDS, points, sd::RECORD_SIZE);
    put_rounds(f, f.standings + sd::TABLE_ROUNDS, table, sd::RECORD_SIZE);
    put_rounds(f, f.standings + RESULTS_ROUNDS, results, RESULT_SIZE);
    return f;
}

static void test_order_and_totals()
{
    Fake f = season();
    Standings s;
    CHECK(read(f.standings, fake_read, &f, s), "a whole season reads");
    CHECK(s.drivers.size() == 3, "every driver in the table is listed");
    CHECK(s.drivers.size() == 3 && (s.drivers[0].key >> 16) == 9 && (s.drivers[1].key >> 16) == 7 &&
              (s.drivers[2].key >> 16) == 5,
          "the drivers come in the order of the table's last session");
    CHECK(s.drivers.size() == 3 && s.drivers[0].points == 58 && s.drivers[1].points == 58 && s.drivers[2].points == 44,
          "each driver's total is the table's");
    CHECK(s.rounds == 2, "the season has two rounds of points");
}

static void test_rounds()
{
    Fake f = season();
    Standings s;
    read(f.standings, fake_read, &f, s);
    const Driver *a = find(s, 7), *c = find(s, 5);
    CHECK(a && a->rounds.size() == 2 && a->rounds[0].points == 33 && a->rounds[1].points == 25,
          "a round's points add up its sessions");
    CHECK(c && c->rounds.size() == 2 && c->rounds[1].points == 23,
          "a session the extra points were copied into counts in its round");
    CHECK(a && a->rounds.size() == 2 && a->rounds[0].finishes.size() == 2 && a->rounds[0].finishes[0].session == 12 &&
              a->rounds[0].finishes[0].position == 1 && a->rounds[0].finishes[1].session == 15 &&
              a->rounds[0].finishes[1].position == 1,
          "each results session gives the driver's 1-based finishing position");
    CHECK(c && c->rounds.size() == 2 && c->rounds[1].finishes.size() == 2 && c->rounds[1].finishes[1].status == 3,
          "a finish keeps the result's status");
    CHECK(a && a->rounds[1].finishes.size() == 2 && a->rounds[1].finishes[0].position == 2, "positions are per session");
}

static void test_key_match()
{
    Fake f;
    uint32_t a = key_of(7, 1, 0), a2 = key_of(7, 1, 4), b = key_of(7, 2, 0);
    put_rounds(f, f.standings + sd::POINTS_ROUNDS, {{{15, {{a2, 25}, {b, 1}}}}}, sd::RECORD_SIZE);
    put_rounds(f, f.standings + sd::TABLE_ROUNDS, {{{15, {{a, 25}, {b, 1}}}}}, sd::RECORD_SIZE);
    put_rounds(f, f.standings + RESULTS_ROUNDS, {{{15, {{a2, 0, 0, 0}, {b, 4, 0, 0}}}}}, RESULT_SIZE);
    Standings s;
    CHECK(read(f.standings, fake_read, &f, s) && s.drivers.size() == 2, "two keys with different kinds are two drivers");
    CHECK(s.drivers.size() == 2 && s.drivers[0].rounds.size() == 1 && s.drivers[0].rounds[0].points == 25 &&
              s.drivers[0].rounds[0].finishes.size() == 1 && s.drivers[0].rounds[0].finishes[0].position == 1,
          "the key's second byte does not split a driver, as in the game's countback");
    CHECK(s.drivers.size() == 2 && s.drivers[1].rounds[0].points == 1 && s.drivers[1].rounds[0].finishes.size() == 1 &&
              s.drivers[1].rounds[0].finishes[0].position == 5,
          "the kind byte does");
}

static void test_empty_and_trailing()
{
    Fake f;
    Standings s;
    CHECK(read(f.standings, fake_read, &f, s) && s.drivers.empty() && s.rounds == 0,
          "a season with nothing raced reads as no drivers");

    Fake g;
    uint32_t a = key_of(3);
    put_rounds(g, g.standings + sd::POINTS_ROUNDS, {{{15, {{a, 10}}}}, {}}, sd::RECORD_SIZE);
    put_rounds(g, g.standings + sd::TABLE_ROUNDS, {{{15, {{a, 10}}}}, {}}, sd::RECORD_SIZE);
    put_rounds(g, g.standings + RESULTS_ROUNDS, {{{15, {{a, 3, 0, 0}}}}, {{15, {{a, 1, 0, 0}}}}}, RESULT_SIZE);
    Standings t;
    CHECK(read(g.standings, fake_read, &g, t) && t.drivers.size() == 1 && t.drivers[0].points == 10,
          "the table's last round with a session gives the order");
    CHECK(t.rounds == 2 && t.drivers[0].rounds.size() == 2 && t.drivers[0].rounds[1].points == 0 &&
              t.drivers[0].rounds[1].finishes.size() == 1 && t.drivers[0].rounds[1].finishes[0].position == 2,
          "a round with results and no points yet keeps its finishes");
}

static void test_short_reads()
{
    Fake f = season();
    uint64_t edge = Fake::BASE + f.bytes.size() - sd::ROUND_SIZE;
    f.put64(f.standings + RESULTS_ROUNDS, edge);
    f.put64(f.standings + RESULTS_ROUNDS + 8, edge + 2 * sd::ROUND_SIZE);
    Standings s;
    CHECK(!read(f.standings, fake_read, &f, s), "results that run off the readable memory fail the read");

    Fake g = season();
    uint64_t b = 0;
    memcpy(&b, &g.bytes[g.standings + sd::TABLE_ROUNDS - Fake::BASE], 8);
    g.put64(g.standings + sd::TABLE_ROUNDS + 8, b + 0x11);
    Standings t;
    CHECK(!read(g.standings, fake_read, &g, t), "a table that is not whole rounds fails the read");

    Fake h = season();
    uint64_t pb = 0;
    memcpy(&pb, &h.bytes[h.standings + sd::POINTS_ROUNDS - Fake::BASE], 8);
    uint64_t sb = 0;
    memcpy(&sb, &h.bytes[pb + 8 - Fake::BASE], 8);
    uint64_t recs = 0;
    memcpy(&recs, &h.bytes[sb + 8 - Fake::BASE], 8);
    h.put64(sb + 16, recs + 3);
    Standings u;
    CHECK(!read(h.standings, fake_read, &h, u), "a points session that is not whole records fails the read");

    Fake k = season();
    uint64_t tb = 0;
    memcpy(&tb, &k.bytes[k.standings + sd::TABLE_ROUNDS - Fake::BASE], 8);
    uint64_t last = tb + sd::ROUND_SIZE;
    uint64_t lsb = 0;
    memcpy(&lsb, &k.bytes[last + 8 - Fake::BASE], 8);
    uint64_t ls = lsb + 2 * sd::SESSION_SIZE;
    uint64_t lrb = 0;
    memcpy(&lrb, &k.bytes[ls + 8 - Fake::BASE], 8);
    k.put64(ls + 16, lrb + (sd::MAX_RECORDS + 1) * sd::RECORD_SIZE);
    Standings v;
    CHECK(!read(k.standings, fake_read, &k, v), "a table session longer than MAX_RECORDS fails the read");
}

static void test_json()
{
    Fake f = season();
    Standings s;
    read(f.standings, fake_read, &f, s);
    std::string j = json(s, 2);
    CHECK(j.rfind("{\"season\":2,\"rounds\":2,\"drivers\":[{\"driver\":9,\"position\":1,\"points\":58,", 0) == 0,
          "the JSON starts with the season, its rounds and the leader");
    CHECK(j.find("{\"driver\":5,\"position\":3,\"points\":44,\"rounds\":[{\"points\":21,\"finishes\":[{\"session\":12,"
                 "\"position\":3,\"status\":0},{\"session\":15,\"position\":3,\"status\":0}]},{\"points\":23,") !=
              std::string::npos,
          "a driver lists his rounds with their points and finishes");
    CHECK(j.size() > 2 && j.compare(j.size() - 4, 4, "]}]}") == 0, "the JSON closes");
    Standings none;
    CHECK(json(none, 0) == "{\"season\":0,\"rounds\":0,\"drivers\":[]}", "no drivers is an empty list");
}

int main()
{
    test_order_and_totals();
    test_rounds();
    test_key_match();
    test_empty_and_trailing();
    test_short_reads();
    test_json();
    if (failures) {
        printf("%d failure(s)\n", failures);
        return 1;
    }
    printf("all standings read checks passed\n");
    return 0;
}
