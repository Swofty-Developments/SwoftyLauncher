// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

namespace f1990 {

template <class T> T at(const void *p, size_t off)
{
    T v;
    std::memcpy(&v, static_cast<const uint8_t *>(p) + off, sizeof v);
    return v;
}

constexpr uint8_t kQualifyingGroup = 1;
constexpr uint8_t kShootoutGroup = 2;
constexpr uint32_t kRace2Session = 16;

// The 1990 weekend: a SprintShootout-group session (Friday) and a Qualifying-group session
// (Saturday), and no race_2 (the stock sprint weekend's grand prix).
inline bool is_1990_weekend(const void *weekend)
{
    auto *s = at<const uint8_t *>(weekend, 0x220), *e = at<const uint8_t *>(weekend, 0x228);
    if (!s || e < s)
        return false;
    bool shootout = false, qualifying = false;
    for (; s < e; s += 0xa0) {
        uint32_t id = at<uint32_t>(s, 0x08);
        uint8_t group = at<uint8_t>(s, 0x98);
        if (id == kRace2Session)
            return false;
        shootout |= group == kShootoutGroup;
        qualifying |= group == kQualifyingGroup;
    }
    return shootout && qualifying;
}

}
