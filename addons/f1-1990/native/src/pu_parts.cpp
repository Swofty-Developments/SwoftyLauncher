// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the part allocation of a 1990 career lists the race engine, the qualifying engine and the gearbox
// only. Its own DLL.
//
// The power-unit panel (garage monitor and laptop) is one view model, built by Sess_PuPanel_BuildComponents: a
// row for each of the seven slots of the driver's EngineManagementComponent record (0 ICE, 1 MGU-K, 2 MGU-H,
// 3 ES, 4 TC, 5 CE, 6 event gearbox) whose holder has a fitted part. At site_Sess_PuPanel_SlotHolder the
// holder of a slot other than 0, 1 and 6 becomes an empty holder, so the game skips that row; rows keep their
// slot index, so fitting and refreshing the three left act on the right slots. data/pu-strings.json renames
// the ICE slot Engine and the MGU-K slot Qualifying Engine through the core's runtime string overrides (API
// v8). Game_EngineMgmt_ApplySessionWear: the qualifying engine slot wears only in the qualifying groups (1
// Qualifying, 2 SprintShootout), the race engine slot only outside them and the hidden slots never (their
// holders' wear ranges are 0 for the call). With pu_rules_1990 on no part wears at all. All of it only while
// a 1990 career is loaded; option pu_parts_1990 turns it off.

#include <atomic>
#include <cstring>
#include <string>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#if SWOFTY_API_VERSION >= 8
#include <swofty/lang.hpp>
#endif

#include "career_1990.hpp"
#include "json_strings.hpp"
#include "pu_parts.hpp"
#include "rnd_progress.hpp"

SWOFTY_SETTING(bool, pu_parts_1990, true);

namespace {

using f1990::at;

constexpr size_t kWeekendVt = 0x98 / 8;
constexpr const char *kSlotNames[] = {"ICE", "MGU-K", "MGU-H", "ES", "TC", "CE", "gearbox"};

alignas(16) const uint8_t g_empty_holder[0x48] = {};
std::atomic<bool> g_1990{false};
std::atomic<uint32_t> g_hidden_logged, g_wear_logged;

int current_group(void *component)
{
    auto *provider = at<void *>(component, 0x270);
    auto **vt = provider && swofty::mem::readable(provider, 8) ? *static_cast<void ***>(provider) : nullptr;
    if (!vt || !swofty::mem::readable(vt + kWeekendVt, 8) || !vt[kWeekendVt])
        return -1;
    auto *weekend = reinterpret_cast<void *(*)(void *)>(vt[kWeekendVt])(provider);
    if (!weekend || !swofty::mem::readable(weekend, 0x240))
        return -1;
    return f1990::session_group(weekend, at<uint32_t>(weekend, 0x238));
}

#if SWOFTY_API_VERSION >= 8
constexpr char kStringsFile[] = "data/pu-strings.json";

struct Override {
    std::string language, key, text;
};

std::vector<Override> g_strings;
bool g_strings_read, g_strings_set;

void read_strings()
{
    std::string json;
    size_t error_at = 0;
    std::vector<f1990::StringLeaf> leaves;
    if (auto r = swofty::storage::read(kStringsFile, json); r != swofty::storage::Result::ok) {
        swofty::log("[f1-1990] parts: read %s: %s", kStringsFile, swofty::storage::name(r));
        return;
    }
    if (!f1990::parse_string_leaves(json, leaves, &error_at)) {
        swofty::log("[f1-1990] parts: %s is not an object of strings (byte %zu)", kStringsFile, error_at);
        return;
    }
    for (auto &l : leaves)
        if (l.path.size() == 3 && l.path[0] == "strings")
            g_strings.push_back({l.path[1], l.path[2], l.text});
}

void part_names(bool on)
{
    namespace lg = swofty::lang;
    if (on == g_strings_set || !lg::available())
        return;
    if (on && !g_strings_read) {
        g_strings_read = true;
        read_strings();
    }
    size_t failed = 0;
    for (auto &s : g_strings)
        failed += (on ? lg::set(s.key, s.text, s.language) : lg::clear(s.key, s.language)) != lg::Result::ok;
    g_strings_set = on;
    swofty::log("[f1-1990] parts: %s %zu part name string(s) (%zu failed)", on ? "set" : "cleared", g_strings.size(),
                failed);
}
#else
void part_names(bool on)
{
    if (on)
        swofty::log("[f1-1990] parts: built against SDK API v%d, without runtime string overrides; the part names "
                    "stay ICE and MGU-K",
                    SWOFTY_API_VERSION);
}
#endif

void on_career(const SwoftyEvent *ev, void *)
{
    bool yes = false;
    if (pu_parts_1990 && !std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED)) {
        void *career = swofty::career::entity();
        yes = career && f1990::is_1990_career(career);
    }
    if (yes != g_1990.load())
        swofty::log("[f1-1990] parts: %s", yes ? "1990 career: engine, qualifying engine and gearbox only"
                                               : "no 1990 career: stock power-unit parts");
    g_1990 = yes;
    part_names(yes);
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] parts: subscribe %s failed; the power-unit parts stay stock", event);
}

struct SavedRange {
    uint8_t *holder;
    float min, max;
};

}

SWOFTY_SITE(site_Sess_PuPanel_SlotHolder, swofty::Context &c)
{
    uint32_t slot = uint32_t(c.rbp);
    if (!g_1990.load() || !pu_parts_1990 || slot >= f1990::kSlotCount || f1990::slot_listed(slot))
        return;
    c.rax = reinterpret_cast<uintptr_t>(g_empty_holder);
    if (g_hidden_logged++ < 12)
        swofty::log("[f1-1990] parts: power-unit panel row %u (%s) hidden", slot, kSlotNames[slot]);
}

SWOFTY_WRAP(Game_EngineMgmt_ApplySessionWear, void, void *engine_management, uint32_t driver, float amount, uint8_t flag)
{
    if (!g_1990.load() || !pu_parts_1990 || !engine_management || !(amount > 0.f) ||
        !swofty::mem::readable(engine_management, 0x2b0))
        return orig(engine_management, driver, amount, flag);
    auto *record = f1990::pu_record(at<uint8_t *>(engine_management, 0x2a0), at<uint8_t *>(engine_management, 0x2a8),
                                    driver);
    if (!record || !swofty::mem::readable(record, f1990::kPuRecordSize))
        return orig(engine_management, driver, amount, flag);
    int group = current_group(engine_management);
    SavedRange saved[f1990::kSlotCount];
    size_t n = 0;
    for (uint32_t slot = 0; slot < f1990::kSlotCount; slot++) {
        if (f1990::slot_wears(slot, group))
            continue;
        auto *holder = at<uint8_t *>(record, f1990::kPuSlotsAt + slot * 0x10);
        if (!holder || !swofty::mem::readable(holder, f1990::kHolderWearMax + 4))
            continue;
        saved[n++] = {holder, at<float>(holder, f1990::kHolderWearMin), at<float>(holder, f1990::kHolderWearMax)};
        std::memset(holder + f1990::kHolderWearMin, 0, 8);
    }
    if (g_wear_logged++ < 16)
        swofty::log("[f1-1990] parts: driver 0x%08x session group %d: wear %.3f on the %s and the gearbox; %zu slot(s) "
                    "without wear",
                    driver, group, double(amount),
                    f1990::slot_wears(f1990::kQualiEngineSlot, group) ? "qualifying engine" : "race engine", n);
    orig(engine_management, driver, amount, flag);
    for (size_t i = 0; i < n; i++) {
        std::memcpy(saved[i].holder + f1990::kHolderWearMin, &saved[i].min, 4);
        std::memcpy(saved[i].holder + f1990::kHolderWearMax, &saved[i].max, 4);
    }
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] parts: 1990 part allocation %s", pu_parts_1990 ? "on" : "off (pu_parts_1990)");
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
}

SWOFTY_ADDON()
