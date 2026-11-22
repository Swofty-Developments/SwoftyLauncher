#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>

namespace runtime {

inline constexpr uint64_t FNV64_BASIS = 0xcbf29ce484222325ull;
inline constexpr uint64_t FNV64_PRIME = 0x100000001b3ull;
inline constexpr uint32_t FNV32_BASIS = 0x811c9dc5u;
inline constexpr uint32_t FNV32_PRIME = 0x01000193u;

inline uint64_t fnv64(const void *p, size_t n, uint64_t h = FNV64_BASIS)
{
    auto b = static_cast<const uint8_t *>(p);
    for (size_t i = 0; i < n; i++)
        h = (h ^ b[i]) * FNV64_PRIME;
    return h;
}

inline uint64_t fnv64(const std::string &s, uint64_t h = FNV64_BASIS) { return fnv64(s.data(), s.size(), h); }

inline uint32_t fnv32(const void *p, size_t n, uint32_t h = FNV32_BASIS)
{
    auto b = static_cast<const uint8_t *>(p);
    for (size_t i = 0; i < n; i++)
        h = (h ^ b[i]) * FNV32_PRIME;
    return h;
}

inline uint32_t fnv32(const std::string &s, uint32_t h = FNV32_BASIS) { return fnv32(s.data(), s.size(), h); }

inline uint32_t fnv32(const char *s)
{
    uint32_t h = FNV32_BASIS;
    for (; *s; s++)
        h = (h ^ uint8_t(*s)) * FNV32_PRIME;
    return h;
}

// FNV-1a over s and one more step, so the fields of a chain stay apart ("ab" then "c" differs from "a" then "bc").
inline uint64_t fnv64_field(uint64_t h, const std::string &s) { return fnv64(s, h) * FNV64_PRIME + 0x9e3779b97f4a7c15ull; }

// Reads eight bytes a step: the checksum and name hash of large blobs, where fnv64's byte loop is slow.
inline uint64_t mix64(const void *p, size_t n)
{
    uint64_t h = 0x9e3779b97f4a7c15ull ^ n;
    auto b = static_cast<const uint8_t *>(p);
    size_t i = 0;
    for (; i + 8 <= n; i += 8) {
        uint64_t v;
        std::memcpy(&v, b + i, 8);
        h ^= v * 0xff51afd7ed558ccdull;
        h = (h << 31 | h >> 33) * 0xc4ceb9fe1a85ec53ull;
    }
    for (; i < n; i++)
        h = (h ^ b[i]) * FNV64_PRIME;
    h ^= h >> 33;
    return h * 0xff51afd7ed558ccdull;
}

void md5(const void *data, size_t n, uint8_t out[16]);
std::string hex16(uint64_t v);

}
