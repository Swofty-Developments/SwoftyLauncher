// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: when a snapshot section is rendered again. Header-only so the host tests include it.

#pragma once
#include <algorithm>
#include <cstdint>

namespace ce::sched {

inline constexpr double kBaseMs = 1000;
inline constexpr double kMaxMs = 30000;
inline constexpr double kSectionFactor = 100;
inline constexpr double kGlobalFactor = 100;
inline constexpr int kMaxBackoff = 3;

inline double section_interval_ms(double cost_ms, int unchanged_streak)
{
    double base = std::max(kBaseMs, cost_ms * kSectionFactor);
    double back = base * double(1 << std::min(std::max(unchanged_streak, 0), kMaxBackoff));
    return std::min(back, kMaxMs);
}

inline double global_gap_ms(double cost_ms)
{
    return std::min(cost_ms * kGlobalFactor, kMaxMs);
}

struct Slot {
    bool rendered = false;
    bool urgent = false;
    double due_ms = 0;
    int unchanged = 0;
};

struct Clock {
    double global_ms = 0;
};

inline bool may_render(const Slot &s, const Clock &c, double now_ms)
{
    if (!s.rendered || s.urgent)
        return true;
    return now_ms >= s.due_ms && now_ms >= c.global_ms;
}

inline void rendered(Slot &s, Clock &c, double now_ms, double cost_ms, bool changed)
{
    bool first = !s.rendered || s.urgent;
    s.urgent = false;
    s.unchanged = changed ? 0 : s.unchanged + 1;
    s.rendered = true;
    s.due_ms = now_ms + section_interval_ms(cost_ms, s.unchanged);
    if (!first)
        c.global_ms = std::max(c.global_ms, now_ms + global_gap_ms(cost_ms));
}

inline void invalidate(Slot &s)
{
    s.due_ms = 0;
    s.unchanged = 0;
    s.urgent = true;
}

inline void invalidate_all(Slot *s, int n, Clock &c)
{
    for (int i = 0; i < n; ++i)
        invalidate(s[i]);
    c.global_ms = 0;
}

} // namespace ce::sched
