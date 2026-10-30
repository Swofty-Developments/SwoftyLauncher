// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

#include "weekend.hpp"

namespace f1990 {

inline constexpr char kCareerGroup1990[] = "f1_career_1990";

struct CareerGroupAccess {
    void *(*setup_component)(void *championship);
    uint8_t (*group_index)(void *component);
};

// The career's data group, as the save-slot predicates 0x1438fb390/0x1438fb5d0 read it: for each
// championship of the career (career +0x1040..+0x1048, 16-byte entries, championship at +8), its
// CareerSetupComponent's data-group index into the group list at (component +0x290) +0x560..+0x568
// (16-byte entries, group at +8, name at *(group +0x28) +0x10).
// What the walk below sees for the first championship with a CareerSetupComponent: its data group
// index, the group list's length and the name at that index (nullptr when out of range).
struct CareerGroupSeen {
    int index = -1;
    size_t count = 0;
    const char *name = nullptr;
};

inline CareerGroupSeen career_data_group_seen(const void *career, const CareerGroupAccess &a)
{
    CareerGroupSeen seen;
    if (!career)
        return seen;
    auto *b = at<const uint8_t *>(career, 0x1040), *e = at<const uint8_t *>(career, 0x1048);
    for (; b && b < e; b += 0x10) {
        void *champ = at<void *>(b, 0x08);
        void *comp = champ ? a.setup_component(champ) : nullptr;
        auto *src = comp ? at<const uint8_t *>(comp, 0x290) : nullptr;
        if (!src)
            continue;
        seen.index = a.group_index(comp);
        auto *lb = at<const uint8_t *>(src, 0x560), *le = at<const uint8_t *>(src, 0x568);
        seen.count = lb && le >= lb ? size_t(le - lb) / 0x10 : 0;
        if (size_t(seen.index) < seen.count) {
            auto *group = at<const uint8_t *>(lb + size_t(seen.index) * 0x10, 0x08);
            auto *named = group ? at<const uint8_t *>(group, 0x28) : nullptr;
            seen.name = named ? at<const char *>(named, 0x10) : nullptr;
        }
        return seen;
    }
    return seen;
}

inline bool career_data_group_is(const void *career, const char *want, const CareerGroupAccess &a)
{
    if (!career || !want)
        return false;
    auto *b = at<const uint8_t *>(career, 0x1040), *e = at<const uint8_t *>(career, 0x1048);
    for (; b && b < e; b += 0x10) {
        void *champ = at<void *>(b, 0x08);
        void *comp = champ ? a.setup_component(champ) : nullptr;
        if (!comp)
            continue;
        auto *src = at<const uint8_t *>(comp, 0x290);
        if (!src)
            continue;
        uint8_t idx = a.group_index(comp);
        auto *lb = at<const uint8_t *>(src, 0x560), *le = at<const uint8_t *>(src, 0x568);
        if (!lb || le < lb || idx >= size_t(le - lb) / 0x10)
            continue;
        auto *group = at<const uint8_t *>(lb + idx * 0x10, 0x08);
        auto *named = group ? at<const uint8_t *>(group, 0x28) : nullptr;
        const char *name = named ? at<const char *>(named, 0x10) : nullptr;
        if (name && !std::strcmp(name, want))
            return true;
    }
    return false;
}

}
