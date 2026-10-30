// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - qualifying grid from the better of two sessions.
//
// The 1990 weekend runs a SprintShootout-group session on Friday (group 2) and a Qualifying-group
// session on Saturday (group 1), with no sprint race. The race grid (grid_type qualifying) reads the
// group-1 classification in Weekend+0x488 (std::map<u8 group, GroupResult>). The game records each
// group's per-driver best lap in GroupResult.times after every qualifying-type session, driven or
// simulated (Sess_Record_GroupTimes), and builds the group rows when the group's last session ends or
// a save loads (Sess_Build_GroupResult). After either call this rewrites the group-1 row positions
// so they rank min(Friday best, Saturday best); drivers without a time follow in Saturday order.
// Weekends without a Friday shootout group, or with a sprint race (race_2), are left alone: that
// weekend structure exists only in the f1_1990 setup mode, so 2025/2026 sessions are untouched.

#include <algorithm>
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <vector>

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include "grid_rank.hpp"
#include "weekend.hpp"

namespace {

using f1990::at;
using f1990::is_1990_weekend;

constexpr const char *kVersion = "0.1.0";

constexpr uint8_t kQualifying = f1990::kQualifyingGroup;
constexpr uint8_t kShootout = f1990::kShootoutGroup;

struct MapNode {
    MapNode *left;
    MapNode *parent;
    MapNode *right;
    uint8_t color;
    uint8_t is_nil;
    uint8_t _pad_0x1a[6];
    uint8_t key;
    uint8_t _pad_0x21[7];
};
static_assert(offsetof(MapNode, key) == 0x20);

struct TimeRecord {
    uint32_t key;
    float time;
};

uint8_t *group_result(void *weekend, uint8_t group)
{
    auto *head = at<MapNode *>(weekend, 0x488);
    if (!head)
        return nullptr;
    MapNode *n = head->parent;
    while (n && !n->is_nil) {
        if (n->key == group)
            return reinterpret_cast<uint8_t *>(n) + 0x28;
        n = group < n->key ? n->left : n->right;
    }
    return nullptr;
}

float time_of(const uint8_t *result, uint32_t key)
{
    auto *b = at<const TimeRecord *>(result, 0x28), *e = at<const TimeRecord *>(result, 0x30);
    for (; b && b < e; ++b)
        if (b->key == key)
            return b->time;
    return 0.f;
}

const char *key_name(uint32_t key, char (&buf)[24])
{
    std::snprintf(buf, sizeof buf, "%u/%u", key & 0xff, key >> 16);
    return buf;
}

void apply(void *weekend, const char *why)
{
    if (!weekend || !is_1990_weekend(weekend))
        return;
    uint8_t *sat = group_result(weekend, kQualifying), *fri = group_result(weekend, kShootout);
    if (!sat || !fri || at<uint8_t *>(sat, 0x08) == at<uint8_t *>(sat, 0x10))
        return;
    auto **pb = at<uint8_t **>(weekend, 0x288), **pe = at<uint8_t **>(weekend, 0x290);
    std::vector<f1990::Driver> drivers;
    std::vector<int32_t *> slots;
    for (; pb && pb < pe; ++pb) {
        const uint8_t *p = *pb;
        if (!p || (p[0x4b] & 0x20))
            continue;
        uint32_t key = at<uint32_t>(p, 0x50);
        int32_t *pos = swofty::game::Sess_Find_GroupRowPosition(weekend, sat, key);
        if (!pos || std::find(slots.begin(), slots.end(), pos) != slots.end())
            continue;
        slots.push_back(pos);
        drivers.push_back({key, *pos, time_of(fri, key), time_of(sat, key)});
    }
    if (drivers.empty())
        return;
    bool any_time = false;
    for (const auto &d : drivers)
        any_time |= f1990::best_time(d) > 0.f;
    if (!any_time) {
        swofty::log("[f1-1990] GRID best-of (%s): no recorded times in either session; Saturday order kept", why);
        return;
    }
    std::vector<int32_t> ranked = f1990::rank(drivers);
    swofty::log("[f1-1990] GRID best-of (%s): %zu drivers", why, drivers.size());
    char name[24];
    for (size_t i = 0; i < drivers.size(); i++) {
        const f1990::Driver &d = drivers[i];
        swofty::log("[f1-1990] GRID driver %s fri %.3f sat %.3f best %.3f: position %d -> %d", key_name(d.key, name),
                    d.friday_time, d.saturday_time, f1990::best_time(d), d.saturday, ranked[i]);
        *slots[i] = ranked[i];
    }
}

}

SWOFTY_WRAP(Sess_Build_GroupResult, void, void *weekend, uint8_t group, void *result)
{
    orig(weekend, group, result);
    if (group == kQualifying || group == kShootout)
        apply(weekend, group == kQualifying ? "qualifying built" : "shootout built");
}

SWOFTY_WRAP(Sess_Record_GroupTimes, void, void *weekend, uint8_t group, void *entries)
{
    orig(weekend, group, entries);
    if (group == kQualifying || group == kShootout)
        apply(weekend, group == kQualifying ? "qualifying times" : "shootout times");
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] loaded v%s: qualifying grid = best lap of the Friday and Saturday sessions", kVersion);
}

