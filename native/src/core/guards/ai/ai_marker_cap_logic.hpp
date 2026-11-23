#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

// Game_AiRoute_CompileMarkerChannel (re/bindings/game/Game_AiRoute_CompileMarkerChannel.json) without its 256-entry
// stack arrays. The game function gathers, per AI lane, the markers of 7 channels (gate +0xc8 + c * 8) into stack
// arrays of 256 distances and 256 pointers each and copies them into the lane's block of the heap buffer that
// Game_AiRoute_BuildGraph sized with Game_AiRoute_CalcLaneBufferSize_Thunk. Its counters have no bound, so a 257th
// marker on one channel overwrites the next channel's arrays and then the saved registers.
//
// count() reads the markers the way the game does; compile() writes the same blocks straight into the game's buffer,
// whose size the game computed from the same count, so no array of its own is needed.
namespace runtime::ai_marker_cap {

constexpr uint32_t CHANNELS = 7;
constexpr uint32_t STOCK_SLOTS = 256;

constexpr uint64_t LANE_LINKS_BEGIN_AT = 0x18;
constexpr uint64_t LANE_LINKS_END_AT = 0x20;
constexpr uint64_t LANE_TABLES_AT = 0x490;
constexpr uint64_t LINK_SPAN_AT = 0x8;
constexpr uint64_t LINK_DISTANCE_AT = 0xb0;
constexpr uint64_t SPAN_FROM_GATE_AT = 0x10;
constexpr uint64_t SPAN_TO_GATE_AT = 0x18;
constexpr uint64_t GATE_MARKERS_AT = 0xc8;

constexpr uint32_t KEY_BELOW = 0xff7fffffu; // -FLT_MAX
constexpr uint32_t KEY_ABOVE = 0x7f7fffffu; // FLT_MAX

// Far above any real lane: Game_AiRoute_BuildGraph numbers a route's links with u16 indices.
constexpr uint64_t MAX_LINKS = 1u << 20;
constexpr int LOG_FIRST = 4;

inline uint64_t load64(uint64_t address)
{
    uint64_t v;
    std::memcpy(&v, reinterpret_cast<const void *>(address), sizeof v);
    return v;
}

inline uint32_t load32(uint64_t address)
{
    uint32_t v;
    std::memcpy(&v, reinterpret_cast<const void *>(address), sizeof v);
    return v;
}

inline void store64(uint64_t address, uint64_t v) { std::memcpy(reinterpret_cast<void *>(address), &v, sizeof v); }

inline void store32(uint64_t address, uint32_t v) { std::memcpy(reinterpret_cast<void *>(address), &v, sizeof v); }

struct Counts {
    // False when the lane's link vector does not look like one; the game's function then runs unchanged.
    bool valid = false;
    // The first link's from gate carries a marker of the channel (the game's per-channel flag).
    bool seeded[CHANNELS] = {};
    uint32_t n[CHANNELS] = {};
    uint64_t links = 0;
};

inline uint64_t gate_marker(uint64_t gate, uint32_t channel) { return load64(gate + GATE_MARKERS_AT + uint64_t(channel) * 8); }

inline uint64_t link_to_gate(uint64_t link) { return load64(load64(link + LINK_SPAN_AT) + SPAN_TO_GATE_AT); }

inline Counts count(uint64_t lane)
{
    Counts c;
    uint64_t begin = load64(lane + LANE_LINKS_BEGIN_AT), end = load64(lane + LANE_LINKS_END_AT);
    if (begin == 0 || end <= begin || (end - begin) % 8 != 0 || (end - begin) / 8 > MAX_LINKS)
        return c;
    uint64_t seed = load64(load64(load64(begin) + LINK_SPAN_AT) + SPAN_FROM_GATE_AT);
    for (uint32_t ch = 0; ch < CHANNELS; ch++) {
        if (gate_marker(seed, ch) != 0) {
            c.seeded[ch] = true;
            c.n[ch] = 1;
        }
    }
    for (uint64_t at = begin; at != end; at += 8) {
        uint64_t to = link_to_gate(load64(at));
        for (uint32_t ch = 0; ch < CHANNELS; ch++)
            c.n[ch] += gate_marker(to, ch) != 0;
    }
    c.links = (end - begin) / 8;
    c.valid = true;
    return c;
}

inline uint32_t most(const Counts &c)
{
    uint32_t m = 0;
    for (uint32_t n : c.n)
        m = n > m ? n : m;
    return m;
}

// The game's arrays hold 256 entries per channel; up to that its own function is right.
inline bool over_stock(const Counts &c) { return c.valid && most(c) > STOCK_SLOTS; }

// One channel's block: {n, -FLT_MAX, n distances, FLT_MAX, a zero when n is even} as u32, then n + 2 pointers
// (the wrap-around predecessor, the n markers, the wrap-around successor). Game_AiRoute_CalcLaneBufferSize_Thunk
// sizes it the same way.
inline uint64_t block_bytes(uint32_t n) { return n ? uint64_t(((n & 1) << 2) ^ 4) + uint64_t(n) * 12 + 0x1c : 0; }

inline uint64_t buffer_bytes(const Counts &c)
{
    uint64_t total = 0;
    for (uint32_t n : c.n)
        total += block_bytes(n);
    return total;
}

inline uint64_t pointers_at(uint64_t block, uint32_t n) { return block + 4 * uint64_t((n & ~1u) + 4); }

struct Result {
    uint64_t bytes = 0;
    // Channels whose markers changed between the two passes; their table is left empty.
    uint32_t dropped = 0;
};

// Writes the lane's tables into out (buffer_bytes(c) bytes) and points lane +0x490 + c * 8 at them, as the game's
// function does: an empty channel gets 0, channels follow each other in order.
inline Result compile(uint64_t lane, uint64_t out, const Counts &c)
{
    Result r;
    uint64_t block[CHANNELS] = {};
    uint32_t k[CHANNELS] = {};
    uint64_t at = out;
    for (uint32_t ch = 0; ch < CHANNELS; ch++) {
        store64(lane + LANE_TABLES_AT + uint64_t(ch) * 8, 0);
        if (c.n[ch]) {
            block[ch] = at;
            at += block_bytes(c.n[ch]);
        }
    }
    r.bytes = at - out;

    auto put = [&](uint32_t ch, uint32_t distance, uint64_t marker) {
        uint32_t n = c.n[ch];
        if (k[ch] >= n)
            return;
        store32(block[ch] + 8 + 4 * uint64_t(k[ch]), distance);
        store64(pointers_at(block[ch], n) + 8 + 8 * uint64_t(k[ch]), marker);
        k[ch]++;
    };

    uint64_t begin = load64(lane + LANE_LINKS_BEGIN_AT), end = begin + c.links * 8;
    uint64_t seed = load64(load64(load64(begin) + LINK_SPAN_AT) + SPAN_FROM_GATE_AT);
    for (uint32_t ch = 0; ch < CHANNELS; ch++) {
        if (c.seeded[ch])
            put(ch, 0, gate_marker(seed, ch));
    }
    for (uint64_t p = begin; p != end; p += 8) {
        uint64_t link = load64(p);
        uint64_t to = link_to_gate(link);
        uint32_t distance = load32(link + LINK_DISTANCE_AT);
        for (uint32_t ch = 0; ch < CHANNELS; ch++) {
            uint64_t marker = gate_marker(to, ch);
            if (marker)
                put(ch, distance, marker);
        }
    }

    for (uint32_t ch = 0; ch < CHANNELS; ch++) {
        uint32_t n = c.n[ch];
        if (!n)
            continue;
        if (k[ch] != n) {
            r.dropped++;
            continue;
        }
        uint64_t b = block[ch], ptrs = pointers_at(b, n);
        store32(b, n);
        store32(b + 4, KEY_BELOW);
        store32(b + 8 + 4 * uint64_t(n), KEY_ABOVE);
        if ((n & 1) == 0)
            store32(b + 12 + 4 * uint64_t(n), 0);
        // The game's choice: on a seeded channel with more than one marker the predecessor of the first is the one
        // before the last and the successor of the last is the second (the seed and the last marker are then
        // presumably the same start gate of a closed lane); otherwise the last and the first.
        bool wraps = c.seeded[ch] && n != 1;
        uint64_t first = ptrs + 8, last = first + 8 * uint64_t(n - 1);
        store64(ptrs, load64(wraps ? last - 8 : last));
        store64(ptrs + 8 + 8 * uint64_t(n), load64(wraps ? first + 8 : first));
        store64(lane + LANE_TABLES_AT + uint64_t(ch) * 8, b);
    }
    return r;
}

}
