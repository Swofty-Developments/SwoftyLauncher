#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>

#include "core/runtime/build.hpp"

namespace runtime {

// A read-only view of one of the game's robin-hood hash maps (keys hashed by multiply and shift).
struct Robin {
    uint8_t *s;

    template <class T> T at(size_t off) const { return *reinterpret_cast<const T *>(s + off); }
    uint64_t mult() const { return at<uint64_t>(0x10); }
    uint64_t *slots() const { return at<uint64_t *>(0x18); }
    uint8_t *info() const { return at<uint8_t *>(0x20); }
    uint64_t count() const { return at<uint64_t>(0x28); }
    uint64_t mask() const { return at<uint64_t>(0x30); }
    uint32_t inc() const { return at<uint32_t>(0x40); }
    uint32_t shift() const { return at<uint32_t>(0x44); }

    size_t buffer() const
    {
        uint64_t n = mask() + 1, a = n < 0x28f5c28f5c28f5dull ? n * 80 / 100 : (n / 100) * 80;
        return size_t(n + std::min<uint64_t>(a, 0xff));
    }

    bool sane() const
    {
        if (!readable(s, 0x48))
            return false;
        uint64_t n = mask() + 1;
        return n >= 8 && !(n & (n - 1)) && inc() >= 1 && inc() <= 0x20 && slots() && info() && readable(info(), 1) &&
               readable(info() + buffer(), 1) && readable(slots(), 16) &&
               readable(reinterpret_cast<uint8_t *>(slots()) + buffer() * 16 - 16, 16);
    }

    uint64_t *find(uint64_t key) const
    {
        uint64_t h = key * mult();
        h ^= h >> 33;
        uint32_t inf = inc() + ((uint32_t(h) & 0x1f) >> (shift() & 0x3f));
        size_t idx = size_t((h >> 5) & mask());
        const uint8_t *in = info();
        uint64_t *sl = slots();
        do {
            if (inf == in[idx] && sl[idx * 2] == key)
                return &sl[idx * 2];
            ++idx;
            inf += inc();
            if (inf == in[idx] && sl[idx * 2] == key)
                return &sl[idx * 2];
            ++idx;
            inf += inc();
        } while (inf <= in[idx]);
        return nullptr;
    }
};

}
