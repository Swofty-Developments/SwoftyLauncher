#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace f1990::heap {

constexpr size_t kHeader = 0x10;

struct Range {
    uintptr_t lo = 0, hi = 0;

    bool holds(const void *p, size_t n) const
    {
        auto a = reinterpret_cast<uintptr_t>(p);
        return a >= lo && a <= hi && n <= hi - a;
    }
};

inline int32_t link(const uint8_t *h, int i)
{
    int32_t v;
    std::memcpy(&v, h + 4 * i, sizeof v);
    return v;
}

inline uint32_t block_size(const uint8_t *h)
{
    uint32_t v;
    std::memcpy(&v, h + 8, sizeof v);
    return v;
}

inline uint32_t requested(const uint8_t *h)
{
    uint32_t v;
    std::memcpy(&v, h + 12, sizeof v);
    return v;
}

inline uint64_t expected_size(uint32_t req, uint64_t guard)
{
    return ((uint64_t(req) + 15) & ~uint64_t(15)) + 2 * guard + kHeader;
}

inline bool allocated(const uint8_t *h, uint64_t guard)
{
    return block_size(h) == expected_size(requested(h), guard);
}

inline uint8_t *target(uint8_t *h, int32_t l)
{
    return l ? h + intptr_t(l) * 16 : nullptr;
}

struct Check {
    uint8_t *older = nullptr, *newer = nullptr;
    bool older_ok = true, newer_ok = true;

    bool ok() const { return older_ok && newer_ok; }
};

inline Check check(uint8_t *h, const Range &r)
{
    Check c;
    c.older = target(h, link(h, 0));
    c.newer = target(h, link(h, 1));
    if (c.older)
        c.older_ok = r.holds(c.older, kHeader) && target(c.older, link(c.older, 1)) == h;
    if (c.newer)
        c.newer_ok = r.holds(c.newer, kHeader) && target(c.newer, link(c.newer, 0)) == h;
    return c;
}

template <class Readable>
uint8_t *predecessor(uint8_t *h, const Range &r, uint64_t guard, size_t max_bytes, Readable readable)
{
    for (size_t d = kHeader; d <= max_bytes; d += kHeader) {
        auto a = reinterpret_cast<uintptr_t>(h);
        if (a < r.lo + d)
            break;
        auto *p = h - d;
        if (!readable(p, kHeader))
            break;
        if (block_size(p) == d && allocated(p, guard))
            return p;
    }
    return nullptr;
}

inline bool repair_older(uint8_t *h, uint8_t *pred)
{
    if (!pred || target(pred, link(pred, 1)) != h)
        return false;
    int32_t v = int32_t((pred - h) / 16);
    std::memcpy(h, &v, sizeof v);
    return true;
}

}
