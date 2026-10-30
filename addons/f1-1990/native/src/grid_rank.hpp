// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <numeric>
#include <vector>

namespace f1990 {

constexpr int32_t kUnclassified = 0x1c;

struct Driver {
    uint32_t key;
    int32_t saturday;
    float friday_time;
    float saturday_time;
};

inline bool has_time(float t)
{
    return std::isfinite(t) && t > 0.f;
}

inline float best_time(const Driver &d)
{
    bool f = has_time(d.friday_time), s = has_time(d.saturday_time);
    if (f && s)
        return std::min(d.friday_time, d.saturday_time);
    return f ? d.friday_time : s ? d.saturday_time : 0.f;
}

inline int32_t base_position(const std::vector<Driver> &drivers)
{
    int32_t base = kUnclassified;
    for (const Driver &d : drivers)
        if (d.saturday >= 0 && d.saturday < kUnclassified)
            base = std::min(base, d.saturday);
    return base == kUnclassified ? 0 : std::min(base, 1);
}

// New group position per driver, in input order. Drivers with a time in either session are ranked by
// the better one (ties keep Saturday order); the rest follow in Saturday order, and a driver with no
// time and no Saturday classification keeps the unclassified position.
inline std::vector<int32_t> rank(const std::vector<Driver> &drivers)
{
    std::vector<size_t> order(drivers.size());
    std::iota(order.begin(), order.end(), size_t{0});
    std::stable_sort(order.begin(), order.end(), [&](size_t a, size_t b) {
        float ta = best_time(drivers[a]), tb = best_time(drivers[b]);
        bool ha = ta > 0.f, hb = tb > 0.f;
        if (ha != hb)
            return ha;
        if (ha && ta != tb)
            return ta < tb;
        return drivers[a].saturday < drivers[b].saturday;
    });
    int32_t base = base_position(drivers);
    std::vector<int32_t> out(drivers.size(), kUnclassified);
    int32_t next = base;
    for (size_t i : order) {
        const Driver &d = drivers[i];
        if (best_time(d) > 0.f || (d.saturday >= 0 && d.saturday < kUnclassified))
            out[i] = next++;
    }
    return out;
}

}
