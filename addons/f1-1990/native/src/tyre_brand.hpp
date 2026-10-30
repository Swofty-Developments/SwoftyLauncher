// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace f1990::brand {

enum class Brand { team, goodyear, pirelli };

constexpr uint32_t kHarder = 0, kBalanced = 1, kSofter = 2;
constexpr char kGoodyearGroup[] = "F1_1990";
constexpr char kPirelliGroup[] = "F1_1990P";

constexpr size_t kChassisGroup = 0x118;
constexpr size_t kStringCapacity = 0x08;
constexpr size_t kStringBuffer = 0x10;

inline Brand for_preset(uint32_t preset)
{
    return preset == kSofter ? Brand::pirelli : preset == kHarder ? Brand::goodyear : Brand::team;
}

inline const char *name(Brand b)
{
    return b == Brand::pirelli ? "Pirelli" : b == Brand::goodyear ? "Goodyear" : "the team's 1990 supplier";
}

inline const char *group(Brand b)
{
    return b == Brand::pirelli ? kPirelliGroup : b == Brand::goodyear ? kGoodyearGroup : nullptr;
}

inline bool is_1990_group(const char *s, size_t capacity)
{
    if (!s || capacity < sizeof kGoodyearGroup)
        return false;
    size_t n = strnlen(s, capacity);
    return n < capacity && (!std::strcmp(s, kGoodyearGroup) || !std::strcmp(s, kPirelliGroup));
}

// Writes `wanted` into a 1990 tyre physics group string buffer in place. -1: not a 1990 group or no room,
// 0: already `wanted`, 1: written.
inline int write_group(char *buf, size_t capacity, const char *wanted)
{
    if (!wanted || !is_1990_group(buf, capacity))
        return -1;
    size_t n = std::strlen(wanted);
    if (n + 1 > capacity)
        return -1;
    if (!std::strcmp(buf, wanted))
        return 0;
    std::memcpy(buf, wanted, n + 1);
    return 1;
}

}
