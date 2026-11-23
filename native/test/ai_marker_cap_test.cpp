// Host-native unit test for the core's AI marker cap guard (native/src/core/guards/ai/ai_marker_cap_logic.hpp).
// Built and run by native/test/run_tests.sh ai_marker_cap.
//
// Builds synthetic AI lanes (link vector, spans, gates with 7 marker slots) in host memory and compares core's
// compile with a transcription of the game's Game_AiRoute_CompileMarkerChannel (re/decomp/140677ae0.c: fixed
// per-channel arrays, the same copy and wrap-around rules) and of Game_AiRoute_CalcLaneBufferSize_Thunk
// (re/decomp/140677910.c).

#include "core/guards/ai/ai_marker_cap_logic.hpp"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <memory>
#include <random>
#include <vector>

namespace amc = runtime::ai_marker_cap;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static uint64_t addr(const void *p) { return reinterpret_cast<uint64_t>(p); }

struct Gate {
    unsigned char bytes[0x100] = {};
};

struct Span {
    unsigned char bytes[0x20] = {};
};

struct Link {
    unsigned char bytes[0xc0] = {};
};

struct Lane {
    unsigned char bytes[0x4d0] = {};
};

// A lane of `links` links over links + 1 gates; gate i is the from gate of link i and the to gate of link i - 1.
struct World {
    std::vector<std::unique_ptr<Gate>> gates;
    std::vector<std::unique_ptr<Span>> spans;
    std::vector<std::unique_ptr<Link>> links;
    std::vector<uint64_t> vec;
    Lane lane;
    uint64_t next_marker = 0x7000'0000'0000ull;

    explicit World(size_t n)
    {
        for (size_t i = 0; i <= n; i++)
            gates.push_back(std::make_unique<Gate>());
        for (size_t i = 0; i < n; i++) {
            spans.push_back(std::make_unique<Span>());
            links.push_back(std::make_unique<Link>());
            amc::store64(addr(spans[i]->bytes) + amc::SPAN_FROM_GATE_AT, addr(gates[i]->bytes));
            amc::store64(addr(spans[i]->bytes) + amc::SPAN_TO_GATE_AT, addr(gates[i + 1]->bytes));
            amc::store64(addr(links[i]->bytes) + amc::LINK_SPAN_AT, addr(spans[i]->bytes));
            float d = 5.0f * float(i + 1);
            std::memcpy(links[i]->bytes + amc::LINK_DISTANCE_AT, &d, 4);
            vec.push_back(addr(links[i]->bytes));
        }
        amc::store64(addr(lane.bytes) + amc::LANE_LINKS_BEGIN_AT, vec.empty() ? 0 : addr(vec.data()));
        amc::store64(addr(lane.bytes) + amc::LANE_LINKS_END_AT, vec.empty() ? 0 : addr(vec.data() + vec.size()));
    }

    void mark(size_t gate, uint32_t ch) { amc::store64(addr(gates[gate]->bytes) + amc::GATE_MARKERS_AT + ch * 8, next_marker++); }
};

// The game's function, as the decomp reads, with Cap entries per channel array (the game: 256).
template <size_t Cap> static void stock_compile(uint64_t lane, uint64_t out)
{
    static uint32_t dist[amc::CHANNELS][Cap];
    static uint64_t ptr[amc::CHANNELS][Cap];
    bool seeded[amc::CHANNELS] = {};
    uint32_t cnt[amc::CHANNELS] = {};
    uint64_t begin = amc::load64(lane + 0x18), end = amc::load64(lane + 0x20);
    uint64_t g0 = amc::load64(amc::load64(amc::load64(begin) + 8) + 0x10);
    for (uint32_t c = 0; c < amc::CHANNELS; c++) {
        uint64_t m = amc::load64(g0 + 0xc8 + c * 8);
        if (m) {
            seeded[c] = true;
            dist[c][0] = 0;
            ptr[c][0] = m;
            cnt[c] = 1;
        }
    }
    for (uint64_t p = begin; p != end; p += 8) {
        uint64_t link = amc::load64(p);
        uint64_t to = amc::load64(amc::load64(link + 8) + 0x18);
        for (uint32_t c = 0; c < amc::CHANNELS; c++) {
            uint64_t m = amc::load64(to + 0xc8 + c * 8);
            if (m) {
                dist[c][cnt[c]] = amc::load32(link + 0xb0);
                ptr[c][cnt[c]] = m;
                cnt[c]++;
            }
        }
    }
    uint32_t *o = reinterpret_cast<uint32_t *>(out);
    for (uint32_t c = 0; c < amc::CHANNELS; c++) {
        amc::store64(lane + 0x490 + c * 8, 0);
        uint32_t n = cnt[c];
        if (!n)
            continue;
        amc::store64(lane + 0x490 + c * 8, addr(o));
        o[0] = n;
        uint64_t q = (n & 0xfffffffe) + 4;
        uint64_t *pp = reinterpret_cast<uint64_t *>(o + q);
        o[1] = 0xff7fffff;
        pp[0] = ptr[c][(~uint32_t(n != 1 && seeded[c])) + n];
        for (uint32_t i = 0; i < n; i++) {
            o[i + 2] = dist[c][i];
            pp[i + 1] = ptr[c][i];
        }
        o[n + 2] = 0x7f7fffff;
        pp[n + 1] = (seeded[c] && n != 1) ? ptr[c][1] : ptr[c][0];
        if ((n & 1) == 0)
            o[n + 3] = 0;
        o = reinterpret_cast<uint32_t *>(reinterpret_cast<unsigned char *>(o) + (((n & 1) << 2) ^ 4) + n * 0xc + 0x1c);
    }
}

// Game_AiRoute_CalcLaneBufferSize_Thunk, as the decomp reads (its protector constants are 0 and 4).
static uint64_t stock_size(uint64_t lane)
{
    uint64_t begin = amc::load64(lane + 0x18), end = amc::load64(lane + 0x20);
    uint64_t g0 = amc::load64(amc::load64(amc::load64(begin) + 8) + 0x10);
    uint32_t n[amc::CHANNELS];
    for (uint32_t c = 0; c < amc::CHANNELS; c++)
        n[c] = amc::load64(g0 + 0xc8 + c * 8) != 0;
    for (uint64_t p = begin; p != end; p += 8) {
        uint64_t to = amc::load64(amc::load64(amc::load64(p) + 8) + 0x18);
        for (uint32_t c = 0; c < amc::CHANNELS; c++)
            n[c] = n[c] + 1 - (amc::load64(to + 0xc8 + c * 8) == 0);
    }
    uint64_t total = 0;
    for (uint32_t c = 0; c < amc::CHANNELS; c++)
        if (n[c])
            total += (~n[c] & 1) * 4 + uint64_t(n[c]) * 0xc + 0x1c;
    return total;
}

constexpr size_t GUARD = 64;

struct Run {
    std::vector<unsigned char> buf;
    uint64_t tables[amc::CHANNELS] = {};
    bool canary_ok = true;
};

// Runs `fn` on a 0x55-filled buffer of `size` bytes (BuildGraph fills it so) with canaries on both sides; table
// pointers are kept relative to the buffer.
template <class Fn> static Run run(World &w, uint64_t size, Fn fn)
{
    Run r;
    std::vector<unsigned char> raw(size + 2 * GUARD, 0xa5);
    std::memset(raw.data() + GUARD, 0x55, size);
    std::memset(w.lane.bytes + amc::LANE_TABLES_AT, 0xcc, amc::CHANNELS * 8);
    uint64_t base = addr(raw.data() + GUARD);
    fn(addr(w.lane.bytes), base);
    for (size_t i = 0; i < GUARD; i++)
        r.canary_ok = r.canary_ok && raw[i] == 0xa5 && raw[GUARD + size + i] == 0xa5;
    r.buf.assign(raw.begin() + GUARD, raw.begin() + GUARD + long(size));
    for (uint32_t c = 0; c < amc::CHANNELS; c++) {
        uint64_t t = amc::load64(addr(w.lane.bytes) + amc::LANE_TABLES_AT + c * 8);
        r.tables[c] = t ? t - base + 1 : 0;
    }
    return r;
}

static bool same(const Run &a, const Run &b)
{
    if (a.buf.size() != b.buf.size() || std::memcmp(a.buf.data(), b.buf.data(), a.buf.size()) != 0)
        return false;
    return std::memcmp(a.tables, b.tables, sizeof a.tables) == 0;
}

// Marks each channel's gates: counts[c] markers, the first on the seed gate when seeds[c].
static void fill(World &w, const uint32_t (&counts)[amc::CHANNELS], const bool (&seeds)[amc::CHANNELS], std::mt19937 &rng)
{
    size_t gates = w.gates.size();
    for (uint32_t c = 0; c < amc::CHANNELS; c++) {
        uint32_t want = counts[c] < gates ? counts[c] : uint32_t(gates);
        std::vector<size_t> pick;
        if (seeds[c] && want)
            pick.push_back(0);
        std::vector<size_t> rest;
        for (size_t g = 1; g < gates; g++)
            rest.push_back(g);
        std::shuffle(rest.begin(), rest.end(), rng);
        for (size_t i = 0; pick.size() < want && i < rest.size(); i++)
            pick.push_back(rest[i]);
        for (size_t g : pick)
            w.mark(g, c);
    }
}

static void case_small(std::mt19937 &rng, int *identical, int *total, bool *stock_kept)
{
    std::uniform_int_distribution<int> links_d(1, 700);
    size_t links = size_t(links_d(rng));
    World w(links);
    uint32_t counts[amc::CHANNELS];
    bool seeds[amc::CHANNELS];
    for (uint32_t c = 0; c < amc::CHANNELS; c++) {
        std::uniform_int_distribution<int> pick(0, 5);
        int k = pick(rng);
        uint32_t cap = uint32_t(links + 1 < 256 ? links + 1 : 256);
        counts[c] = k == 0 ? 0 : k == 1 ? 1 : k == 2 ? 2 : k == 3 ? cap : uint32_t(rng() % (cap + 1));
        seeds[c] = rng() & 1;
    }
    fill(w, counts, seeds, rng);
    amc::Counts c = amc::count(addr(w.lane.bytes));
    uint64_t size = stock_size(addr(w.lane.bytes));
    Run a = run(w, size, [](uint64_t lane, uint64_t out) { stock_compile<256>(lane, out); });
    Run b = run(w, size, [&](uint64_t lane, uint64_t out) { amc::compile(lane, out, c); });
    (*total)++;
    if (same(a, b) && a.canary_ok && b.canary_ok && amc::buffer_bytes(c) == size)
        (*identical)++;
    if (amc::over_stock(c))
        *stock_kept = false;
}

int main()
{
    std::mt19937 rng(0x1406'77ae);

    {
        int identical = 0, total = 0;
        bool stock_kept = true;
        for (int i = 0; i < 2000; i++)
            case_small(rng, &identical, &total, &stock_kept);
        std::printf("      %d of %d lanes with <= 256 markers per channel identical\n", identical, total);
        expect(identical == total, "<= 256 per channel: core's tables are byte-identical to the game's, same size");
        expect(stock_kept, "<= 256 per channel: the guard leaves the lane to the game's function");
    }

    {
        // Edge counts around 256 and odd/even, with and without a seed.
        int bad = 0;
        for (uint32_t n : {1u, 2u, 3u, 255u, 256u}) {
            for (bool seed : {false, true}) {
                World w(400);
                uint32_t counts[amc::CHANNELS] = {n, n, n, n, 0, n, n};
                bool seeds[amc::CHANNELS] = {seed, seed, seed, seed, seed, seed, seed};
                fill(w, counts, seeds, rng);
                amc::Counts c = amc::count(addr(w.lane.bytes));
                uint64_t size = stock_size(addr(w.lane.bytes));
                Run a = run(w, size, [](uint64_t lane, uint64_t out) { stock_compile<256>(lane, out); });
                Run b = run(w, size, [&](uint64_t lane, uint64_t out) { amc::compile(lane, out, c); });
                bad += !same(a, b) || amc::over_stock(c) || c.n[0] != n;
            }
        }
        expect(bad == 0, "1, 2, 3, 255, 256 markers, seeded or not: identical and left to the game");
    }

    {
        World w(256);
        uint32_t counts[amc::CHANNELS] = {0, 257, 0, 0, 0, 0, 0};
        bool seeds[amc::CHANNELS] = {false, true, false, false, false, false, false};
        fill(w, counts, seeds, rng);
        amc::Counts c = amc::count(addr(w.lane.bytes));
        expect(c.valid && c.n[1] == 257 && amc::over_stock(c), "257 brake lines on one lane: over the game's arrays");
    }

    {
        // The Nordschleife case and beyond: the game's arrays would overflow; core's tables must equal the same
        // transcription with arrays large enough, fit the game's own buffer size exactly and write nothing outside it.
        int bad = 0, cases = 0;
        for (uint32_t n6 : {257u, 300u, 548u, 1200u, 4000u}) {
            for (bool seed : {false, true}) {
                World w(4200);
                uint32_t counts[amc::CHANNELS] = {180, n6 > 300 ? 300 : n6, 90, 90, 0, 3, n6};
                bool seeds[amc::CHANNELS] = {seed, false, seed, false, false, seed, seed};
                fill(w, counts, seeds, rng);
                amc::Counts c = amc::count(addr(w.lane.bytes));
                uint64_t size = stock_size(addr(w.lane.bytes));
                Run a = run(w, size, [](uint64_t lane, uint64_t out) { stock_compile<8192>(lane, out); });
                Run b = run(w, size, [&](uint64_t lane, uint64_t out) {
                    amc::Result r = amc::compile(lane, out, c);
                    bad += r.bytes != size || r.dropped != 0;
                });
                cases++;
                bad += !same(a, b) || !b.canary_ok || !amc::over_stock(c) || c.n[6] != n6;
            }
        }
        std::printf("      %d lanes with 257..4000 markers on channel 6\n", cases);
        expect(bad == 0, "> 256 per channel: core's tables equal the unbounded layout, fill the game's buffer exactly");
    }

    {
        // The table layout readers rely on, checked directly on a 548-marker channel.
        World w(600);
        uint32_t counts[amc::CHANNELS] = {0, 0, 0, 0, 0, 0, 548};
        bool seeds[amc::CHANNELS] = {false, false, false, false, false, false, true};
        fill(w, counts, seeds, rng);
        amc::Counts c = amc::count(addr(w.lane.bytes));
        std::vector<unsigned char> buf(amc::buffer_bytes(c) + 16, 0x55);
        uint64_t out = (addr(buf.data()) + 15) & ~uint64_t(15);
        amc::compile(addr(w.lane.bytes), out, c);
        uint64_t t = amc::load64(addr(w.lane.bytes) + amc::LANE_TABLES_AT + 6 * 8);
        bool ok = t == out && amc::load32(t) == 548 && amc::load32(t + 4) == amc::KEY_BELOW &&
                  amc::load32(t + 8 + 4 * 548) == amc::KEY_ABOVE && amc::load32(t + 12 + 4 * 548) == 0;
        float prev = -1.0f;
        for (uint32_t i = 0; i < 548 && ok; i++) {
            uint32_t bits = amc::load32(t + 8 + 4 * i);
            float f;
            std::memcpy(&f, &bits, 4);
            ok = f > prev || (i == 0 && f == 0.0f);
            prev = f;
        }
        uint64_t ptrs = amc::pointers_at(t, 548);
        ok = ok && (ptrs & 7) == 0 && amc::load64(ptrs) == amc::load64(ptrs + 8 + 8 * 546) &&
             amc::load64(ptrs + 8 + 8 * 548) == amc::load64(ptrs + 16);
        for (uint32_t ch = 0; ch < 6; ch++)
            ok = ok && amc::load64(addr(w.lane.bytes) + amc::LANE_TABLES_AT + ch * 8) == 0;
        expect(ok, "548 width-class markers: count, sentinels, rising keys, aligned pointers, wrap entries");
    }

    {
        World w(0);
        amc::Counts c = amc::count(addr(w.lane.bytes));
        expect(!c.valid && !amc::over_stock(c), "a lane with no links is left to the game's function");
    }

    expect(amc::block_bytes(0) == 0 && amc::block_bytes(1) == 40 && amc::block_bytes(2) == 56 && amc::block_bytes(548) == 6608,
           "block sizes as Game_AiRoute_CalcLaneBufferSize_Thunk");

    if (failed) {
        std::printf("%d failed\n", failed);
        return 1;
    }
    std::printf("all passed\n");
    return 0;
}
