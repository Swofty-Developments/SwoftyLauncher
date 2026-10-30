// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the mid-season B-spec car. Its own DLL, so a binding it can't resolve drops only this.
//
// Each 1990 team has two vehicle rows: `<token>_1990` (package f1_1990) and its B-spec copy
// `<token>_1990b` (package f1_1990b, the same car folder and so the same AI data, its own model
// archive). The build gives both rows free stat ids, so the A -> B pairs are read from
// entrants.vehicles by label on career.loaded. The B-spec car is an R&D milestone: an upgrade whose
// `upgrade_name` starts with `f1990_bspec` (the R&D lane's 1990 trees, /mnt/work/.f1-1990/v2/
// rnd-design.md). Once that upgrade's current state is Completed for a team, this plugin points the
// team's career entrant rows at the B-spec vehicle, so from the next session on its cars, AI or
// human, load the `_1990b` vehicle. The entrant rows are the career's own line-up (career entity
// +0x2e0..+0x2e8, 0x88 each; the session grid is built from them) and are saved with the career,
// so the swap persists.
//
// When: the core's career.loaded event and career.hub entered (the hub sits between sessions), only while
// the loaded career's data group is f1_career_1990, and only for rows whose vehicle is an A-spec
// 1990 car. How: the game's own line-up writer, Career_Set_EntrantData_Thunk, with every field of
// the row kept except the vehicle (the career editor's driver swap uses the same writer and the
// same identity rules). Option bspec_cars turns it off.

#include <atomic>
#include <cstring>
#include <string>
#include <unordered_map>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/db.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "career_1990.hpp"

SWOFTY_SETTING(bool, bspec_cars, true);

namespace {

using namespace swofty::game;
using f1990::at;

constexpr char kBspecPrefix[] = "f1990_bspec";
constexpr char kPackageA[] = "f1_1990", kPackageB[] = "f1_1990b";
constexpr uint32_t kNoKey = 0x3fff0000u;
constexpr uint32_t kHumanBit = 0x10000000u;
constexpr uint8_t kCompleted = 3;
constexpr size_t kMaxEntrants = 256, kMaxTeams = 64, kMaxInstances = 0x180, kMaxStates = 0x1000;
constexpr size_t kIdentitySize = offsetof(EntrantRecord, id_lo);
static_assert(sizeof(EntrantRecord) == 0x88);
static_assert(sizeof(UpgradeInstance) == 0xe0);
static_assert(sizeof(UpgradeStateEntry) == 0x10);

std::atomic<uint32_t> g_logged{0};
std::unordered_map<uint32_t, uint32_t> g_a_to_b;
bool g_pairs_read = false;

void read_pairs()
{
    if (g_pairs_read || !swofty::db::available())
        return;
    int64_t count = 0;
    if (swofty::db::int_at("entrants.vehicles._vehicle_instancecount_", &count) != swofty::db::Result::ok || count <= 0 ||
        count > 4096) {
        swofty::log("[f1-1990] bspec: entrants.vehicles is not readable; no B-spec swaps");
        return;
    }
    std::unordered_map<std::string, uint32_t> a, b;
    for (int64_t i = 0; i < count; i++) {
        std::string row = "entrants.vehicles.vehicle_" + std::to_string(i);
        const char *package = nullptr, *label = nullptr;
        int64_t stat = -1;
        if (swofty::db::string_at(row + ".vehicle_package", &package) != swofty::db::Result::ok || !package ||
            swofty::db::string_at(row + "._metadata_", &label) != swofty::db::Result::ok || !label ||
            swofty::db::int_at(row + ".stat_id", &stat) != swofty::db::Result::ok || stat < 0 || stat > 0xffff)
            continue;
        if (!std::strcmp(package, kPackageA))
            a[label] = uint32_t(stat);
        else if (!std::strcmp(package, kPackageB))
            b[label] = uint32_t(stat);
    }
    for (auto &[label, stat] : a) {
        auto it = b.find(label + "b");
        if (it != b.end())
            g_a_to_b[stat] = it->second;
        else
            swofty::log("[f1-1990] bspec: %s has no %sb row; that team keeps its A-spec car", label.c_str(), label.c_str());
    }
    g_pairs_read = true;
    swofty::log("[f1-1990] bspec: %zu A-spec vehicle(s) paired with their B-spec row by label", g_a_to_b.size());
}

uint32_t bspec_of(uint32_t vehicle)
{
    auto it = g_a_to_b.find(vehicle);
    return it == g_a_to_b.end() ? 0 : it->second;
}

bool readable(const void *p, size_t n)
{
    return p && swofty::mem::readable(p, n);
}

bool same_key(uint32_t a, uint32_t b)
{
    return (a & 0xffff00ffu) == (b & 0xffff00ffu);
}

uint32_t stat_of(uint32_t key)
{
    return key >> 16;
}

const UpgradeStateEntry *current_state(const UpgradeInstance &u)
{
    auto *b = u.states_begin, *e = u.states_end;
    if (!b || e <= b || size_t(e - b) > kMaxStates || !readable(e - 1, sizeof(UpgradeStateEntry)))
        return nullptr;
    return e - 1;
}

bool is_bspec(const UpgradeInstance &u)
{
    const char *name = u.name;
    return name && readable(name, sizeof kBspecPrefix) && !std::strncmp(name, kBspecPrefix, sizeof kBspecPrefix - 1);
}

// Team keys whose B-spec upgrade is Completed.
std::vector<uint32_t> completed_teams(uint32_t &teams_seen, uint32_t &bspec_seen)
{
    std::vector<uint32_t> out;
    auto *comp = static_cast<UpgradesComponent *>(swofty::career::component("k_Career_UpgradesComponentDesc"));
    if (!readable(comp, sizeof(UpgradesComponent)))
        return out;
    Upgrades **b = comp->upgrades_begin, **e = comp->upgrades_end;
    if (!b || e < b || size_t(e - b) > kMaxTeams || !readable(b, size_t(e - b) * sizeof(Upgrades *)))
        return out;
    for (; b < e; ++b) {
        Upgrades *up = *b;
        if (!readable(up, sizeof(Upgrades)))
            continue;
        ++teams_seen;
        uint64_t trees = up->tree_count > 2 ? 2 : up->tree_count;
        bool done = false;
        for (uint64_t t = 0; t < trees && !done; ++t) {
            UpgradeInstance *ib = up->trees[t].instances_begin, *ie = up->trees[t].instances_end;
            if (!ib || ie < ib || size_t(ie - ib) > kMaxInstances || !readable(ib, size_t(ie - ib) * sizeof(UpgradeInstance)))
                continue;
            for (; ib < ie; ++ib) {
                if (!is_bspec(*ib))
                    continue;
                ++bspec_seen;
                const UpgradeStateEntry *s = current_state(*ib);
                if (s && s->state == kCompleted) {
                    done = true;
                    break;
                }
            }
        }
        if (done)
            out.push_back(up->team_key);
    }
    return out;
}

bool unique_identity(const EntrantRecord *rows, size_t n, size_t i)
{
    uint32_t id = rows[i].ordinal_flags;
    if (id & kHumanBit) {
        size_t humans = 0;
        for (size_t k = 0; k < n; ++k)
            humans += (rows[k].ordinal_flags & kHumanBit) != 0;
        return humans == 1;
    }
    size_t hits = 0;
    for (size_t k = 0; k < n; ++k)
        hits += !(rows[k].ordinal_flags & kHumanBit) && rows[k].ordinal_flags == id;
    return hits == 1;
}

int swap_rows(void *entity, const std::vector<uint32_t> &teams)
{
    auto *ce = static_cast<CareerEntity *>(entity);
    int swapped = 0;
    for (size_t guard = 0; guard < kMaxEntrants; ++guard) {
        EntrantRecord *b = ce->entrants_begin, *e = ce->entrants_end;
        if (!b || e < b || size_t(e - b) > kMaxEntrants || !readable(b, size_t(e - b) * sizeof(EntrantRecord)))
            return swapped;
        if (ce->entrant_update_busy)
            return swapped;
        size_t n = size_t(e - b), pick = n;
        for (size_t i = 0; i < n; ++i) {
            if ((b[i].vehicle_ref & 0xffff0000u) == kNoKey || !bspec_of(stat_of(b[i].vehicle_ref)))
                continue;
            bool team_done = false;
            for (uint32_t t : teams)
                team_done |= same_key(t, b[i].team_stat_id);
            if (team_done) {
                pick = i;
                break;
            }
        }
        if (pick == n)
            return swapped;
        EntrantRecord row = b[pick];
        if (!unique_identity(b, n, pick)) {
            swofty::log("[f1-1990] bspec: entrant %zu (team %u) has no unique identity; left on the A-spec car", pick,
                        stat_of(row.team_stat_id));
            return swapped;
        }
        uint32_t from = stat_of(row.vehicle_ref), to = bspec_of(from);
        EntrantWriteData d{};
        std::memcpy(&d.driver, &row.id_lo, sizeof d.driver);
        d.driver_customisation = row.driver_customisation;
        d.team_stat_id = row.team_stat_id;
        d.vehicle_ref = (row.vehicle_ref & 0xffffu) | (to << 16);
        d.driver_reg_value = row.driver_reg_value;
        d.vehicle_customisation = row.vehicle_customisation;
        d.badge_stat_id = row.badge_stat_id;
        d.seat_value = row.const_ffff0012;
        uint8_t id[kIdentitySize];
        std::memcpy(id, &row, kIdentitySize);
        Career_Set_EntrantData_Thunk(entity, id, reinterpret_cast<const uint32_t *>(&d), row.team_position, row.car_number);
        EntrantRecord *nb = ce->entrants_begin;
        uint32_t now = (nb && readable(nb + pick, sizeof(EntrantRecord))) ? stat_of(nb[pick].vehicle_ref) : 0;
        swofty::log("[f1-1990] bspec: team %u car #%u: vehicle %u -> %u (%s)", stat_of(row.team_stat_id), row.car_number,
                    from, to, now == to ? "written" : "NOT written");
        if (now != to)
            return swapped;
        ++swapped;
    }
    return swapped;
}

void on_career(const SwoftyEvent *ev, void *)
{
    if (!bspec_cars || (!std::strcmp(ev->name, SWOFTY_EVENT_CAREER_HUB) && ev->a != 1))
        return;
    void *career = swofty::career::entity();
    if (!career || !f1990::is_1990_career(career))
        return;
    read_pairs();
    if (g_a_to_b.empty())
        return;
    uint32_t teams_seen = 0, bspec_seen = 0;
    std::vector<uint32_t> done = completed_teams(teams_seen, bspec_seen);
    int swapped = done.empty() ? 0 : swap_rows(career, done);
    if (swapped || g_logged++ < 4)
        swofty::log("[f1-1990] bspec: %s: %u team R&D model(s), %u B-spec upgrade(s), %zu completed, %d entrant(s) moved "
                    "to the B-spec car",
                    ev->name, teams_seen, bspec_seen, done.size(), swapped);
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    swofty::events::Result r = swofty::events::subscribe(event, on_career, nullptr, &token);
    if (r != swofty::events::Result::ok)
        swofty::log("[f1-1990] bspec: subscribe %s: %s; no B-spec swaps", event, swofty::events::name(r));
}

}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] bspec: B-spec car swap %s", bspec_cars ? "on" : "off (bspec_cars)");
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_HUB);
}

SWOFTY_ADDON()
