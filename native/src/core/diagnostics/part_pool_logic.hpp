#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>

// The model part pool log (part_pool.cpp). Ne_BitAllocator_AllocBit (Steam 0x311ac0) hands out bit indexes from a bitmap:
// [obj+0x8] bitmap begin, [obj+0x10] bitmap end (capacity = bytes * 8 bits), [obj+0x20] the cursor the next scan starts
// at, [obj+0x24] the highest index handed out (cmovle with the new index), [obj+0x58] 0 = a full pool aborts, [obj+0x59]
// the abort flag. The render model resource callback (Steam 0x3d3c640) takes one bit per model part from the object at
// [this+0x20], so a full grid with many add-on models fills it: 94,208 bits on Steam 1.0.145, and when every bit is set
// the game aborts in Ne_FatalAbort (F1_25.exe+0x391a834 < +0x311b55 < +0x3d3d84a). capture() reads the object with a
// read that fails instead of faulting, and counts the set bits of the bitmap.
namespace runtime::part_pool {

constexpr uint64_t kBegin = 0x8;
constexpr uint64_t kEnd = 0x10;
constexpr uint64_t kCursor = 0x20;
constexpr uint64_t kHigh = 0x24;
constexpr uint64_t kAbort = 0x59;

constexpr uint64_t kMaxBytes = 1u << 20;

using Read = bool (*)(uint64_t at, void *out, size_t n);

enum class Verdict : uint8_t { ok, unread, shape };

struct Reading {
    Verdict verdict = Verdict::unread;
    uint64_t object = 0;
    uint64_t capacity = 0;
    uint64_t used = 0;
    uint32_t cursor = 0;
    uint32_t high = 0;
    bool aborted = false;
};

inline uint64_t popcount(const uint8_t *p, size_t n)
{
    uint64_t c = 0;
    for (size_t i = 0; i < n; i++)
        for (uint8_t b = p[i]; b; b &= uint8_t(b - 1))
            c++;
    return c;
}

constexpr bool shaped(uint64_t begin, uint64_t end, uint32_t cursor, uint32_t high)
{
    if (!begin || end <= begin)
        return false;
    uint64_t bytes = end - begin;
    if (bytes > kMaxBytes)
        return false;
    uint64_t capacity = bytes * 8;
    return cursor < capacity && high < capacity;
}

inline Reading capture(Read read, uint64_t object)
{
    Reading r;
    r.object = object;
    if (!object || !read)
        return r;
    uint64_t begin = 0, end = 0;
    uint32_t cursor = 0, high = 0;
    uint8_t abort = 0;
    if (!read(object + kBegin, &begin, sizeof begin) || !read(object + kEnd, &end, sizeof end) ||
        !read(object + kCursor, &cursor, sizeof cursor) || !read(object + kHigh, &high, sizeof high) ||
        !read(object + kAbort, &abort, sizeof abort))
        return r;
    if (!shaped(begin, end, cursor, high)) {
        r.verdict = Verdict::shape;
        return r;
    }
    size_t bytes = size_t(end - begin);
    constexpr size_t kChunk = 256;
    uint8_t buf[kChunk];
    uint64_t used = 0;
    for (size_t at = 0; at < bytes; at += kChunk) {
        size_t n = bytes - at < kChunk ? bytes - at : kChunk;
        if (!read(begin + at, buf, n))
            return r;
        used += popcount(buf, n);
    }
    r.verdict = Verdict::ok;
    r.capacity = uint64_t(bytes) * 8;
    r.used = used;
    r.cursor = cursor;
    r.high = high;
    r.aborted = abort != 0;
    return r;
}

inline int line(const Reading &r, const char *when, char *out, size_t cap)
{
    if (r.verdict == Verdict::ok)
        return std::snprintf(out, cap, "[render] model part pool: %llu of %llu slots used (high-water %u)%s at %s",
                             (unsigned long long)r.used, (unsigned long long)r.capacity, r.high,
                             r.aborted ? ", abort flag set" : "", when);
    if (!r.object)
        return std::snprintf(out, cap, "[render] model part pool: not seen yet at %s (no part has been allocated)", when);
    return std::snprintf(out, cap, "[render] model part pool: object 0x%llx %s at %s", (unsigned long long)r.object,
                         r.verdict == Verdict::shape ? "does not look like the allocator" : "is not readable", when);
}

}
