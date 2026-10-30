// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

#include "weekend.hpp"

namespace f1990 {

// Career_Calc_UpgradeTypeProgress for one upgrade type below 11 (no durability term), with the
// instances skip() names left out of the completed sum only: the weighting (+0x64) of the type's
// (+0x70) instances whose last state (states_end +0xc8, state at entry +0x08) is `state`, over the
// weighting of all the type's instances, times the type's scale (meta entries of 0x20 bytes: type
// +0x00, scale +0x1c; 1 without one), clamped to 0..1. The instance list is up to 0x180 pointers
// with the count at +0xc00.
constexpr size_t kInstanceListMax = 0x180;

template <class Skip>
float type_progress(const void *const *instances, size_t count, uint8_t type, uint8_t state, const uint8_t *meta_begin,
                    const uint8_t *meta_end, Skip skip)
{
    float all = 0.f, done = 0.f;
    for (size_t i = 0; i < count && i < kInstanceListMax; i++) {
        const void *inst = instances[i];
        if (!inst || at<uint8_t>(inst, 0x70) != type)
            continue;
        float w = at<float>(inst, 0x64);
        all += w;
        auto *b = at<const uint8_t *>(inst, 0xc0), *e = at<const uint8_t *>(inst, 0xc8);
        if (!skip(inst) && b && e > b && at<uint8_t>(e - 0x10, 0x08) == state)
            done += w;
    }
    float p = all != 0.f ? done / all : 0.f;
    float scale = 1.f;
    for (const uint8_t *m = meta_begin; m && m + 0x20 <= meta_end; m += 0x20)
        if (m[0] == type) {
            scale = at<float>(m, 0x1c);
            break;
        }
    p *= scale;
    return p < 0.f ? 0.f : p > 1.f ? 1.f : p;
}

constexpr uint32_t kQualiRevLimit = 0x1305fa1e, kQualiEngine = 0x4e03ba09;
constexpr char kQualiPrefix[] = "f1990_quali";

// The qualifying engine upgrades (rnd lane: upgrade names f1990_quali_*, node hashes djb2 of
// "<name>_255" at +0x58).
inline bool is_quali_upgrade(const void *inst)
{
    uint32_t hash = at<uint32_t>(inst, 0x58);
    if (hash == kQualiRevLimit || hash == kQualiEngine)
        return true;
    auto *name = at<const char *>(inst, 0x10);
    return name && !std::strncmp(name, kQualiPrefix, sizeof kQualiPrefix - 1);
}

// A qualifying session of the weekend's session list (+0x220..+0x228, 0xa0 each, group +0x98):
// group 1 (Qualifying) or 2 (SprintShootout, the 1990 Friday session).
inline int session_group(const void *weekend, uint32_t index)
{
    auto *b = at<const uint8_t *>(weekend, 0x220), *e = at<const uint8_t *>(weekend, 0x228);
    if (!b || e <= b || index >= size_t(e - b) / 0xa0)
        return -1;
    return at<uint8_t>(b + size_t(index) * 0xa0, 0x98);
}

inline bool is_qualifying_group(int group)
{
    return group == kQualifyingGroup || group == kShootoutGroup;
}

}
