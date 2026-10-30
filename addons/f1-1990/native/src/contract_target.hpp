// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <algorithm>
#include <cstdint>

namespace f1990 {

struct RatingRange {
    uint32_t min;
    uint32_t max;
    uint32_t minus;
    uint32_t plus;
    int32_t recommended;
};
static_assert(sizeof(RatingRange) == 20);

// The target the contract screen recommends for a driver rated `base`: base plus the first matching
// range's offset, kept inside the screen's slider (base - minus .. base + plus) and at most 100.
inline int32_t recommended_target(int32_t base, const RatingRange *begin, const RatingRange *end)
{
    if (base < 0)
        base = 0;
    for (const RatingRange *r = begin; r && r < end; ++r) {
        if (uint32_t(base) < r->min || uint32_t(base) > r->max)
            continue;
        int64_t lo = std::max<int64_t>(0, int64_t(base) - r->minus);
        int64_t hi = std::min<int64_t>(100, int64_t(base) + r->plus);
        int64_t t = int64_t(base) + r->recommended;
        return int32_t(std::clamp<int64_t>(t, std::min(lo, hi), hi));
    }
    return std::min(base, 100);
}

}
