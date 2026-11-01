// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: reads the gamedb name tables a slice at a time. Header-only so the host tests include it.

#pragma once
#include <algorithm>
#include <cstdint>

namespace ce::names {

inline constexpr int kStages = 3;
inline constexpr int64_t kMaxRows = 4096;
inline constexpr int kRetryEvery = 30;

struct Load {
    int stage = 0;
    int64_t row = 0;
    int64_t count = -1;
    int tries = 0;
    double row_ms = 0;
    bool loaded = false;
    bool settled = false;
    bool reported = false;
};

template <class Source> bool pending(const Load &l, const Source &src)
{
    return src.available() && !l.settled;
}

template <class Source, class Now> bool step(Load &l, Source &src, Now now, double budget_ms)
{
    if (l.loaded || !src.available())
        return false;
    if (l.stage == 0 && l.count < 0 && l.tries++ % kRetryEvery != 0)
        return false;
    double t0 = now();
    while (l.stage < kStages) {
        if (l.count < 0) {
            int64_t count = src.count(l.stage);
            l.count = count > 0 && count <= kMaxRows ? count : 0;
            l.row = 0;
        }
        while (l.row < l.count) {
            double r0 = now();
            src.row(l.stage, l.row++);
            double t1 = now();
            l.row_ms = std::max(t1 - r0, l.row_ms * 0.95);
            if (t1 - t0 + l.row_ms >= budget_ms)
                return false;
        }
        ++l.stage;
        l.count = -1;
    }
    l.stage = 0;
    l.row = 0;
    l.loaded = src.filled();
    l.settled = true;
    if (l.loaded)
        l.reported = false;
    src.finished(l.loaded);
    return true;
}

template <class Source, class Now> int idle(Load &l, Source &src, Now now, double slice_ms)
{
    if (!src.available())
        return 0;
    if (!l.loaded)
        step(l, src, now, slice_ms);
    if (l.settled && !l.reported) {
        l.reported = true;
        return 2;
    }
    return l.loaded ? 0 : 1;
}

} // namespace ce::names
