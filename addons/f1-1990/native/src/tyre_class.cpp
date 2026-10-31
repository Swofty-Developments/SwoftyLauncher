// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the 1990 compound class for the cars of a 1990 career (its own DLL, so a binding it can't
// resolve drops only this hook).
//
// A car's tyre component takes its compound class from the car's "compound class" attribute when the car
// is created: TyreComponent init (0x142ea08f0) calls Sess_TyreClass_FromName, stores the class at +0x9bc0
// and builds its tyre models from the class's visual compounds. The 1990 cars, built on the 2026 asset
// groups, name f1_26, so the garage tyre picker, the HUD, MFD, pause and timing tower tyre widgets and the
// tyre sidewalls showed the 2026 compounds (red S for the Supersoft sets, yellow M for the Soft sets).
// While a 1990 career is loaded, a lookup that returns a class giving every compound option the same
// physical compounds as f1_1990 returns f1_1990 instead, before the models exist; the physical compounds,
// and with them the tyre physics of the car's TyrePhysicsGroup, stay the same.
//
// The component builds three models per entry of the class's visual compound list (+0x170, at most 11
// fit) and maps a visual compound to its model slot through the class's map (+0x1d8, 0xff for a visual
// the class doesn't hold). Showroom and team-select cars show the class's frontend compound (+0x10 dry,
// +0x18 wet; 0x14d42ede0 -> 0x14f4935c0): a class whose frontend compound is not in its own list gives
// slot 0xff, and setWingVisibilityOnCar (0x14f3c6a80) then enters a critical section past the models.
// The swap only happens when both frontend compounds of f1_1990 are in its list and the list fits.
// A Grand Prix of the F1 1990 box has no career: its session takes the f1_1990 class from the setup mode's
// rows while the cars still name f1_26, so every visual compound the session hands out was one their class
// maps to slot 0xff and the tyres drew from garbage. The swap applies there too, when the setup's selected
// entrant list is the box's f1_1990_gp or f1_1990_tt. Setting tyre_class_fix turns it off.

#include <atomic>
#include <bitset>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <mutex>
#include <string>

#include <swofty/career.hpp>
#include <swofty/db.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "box_gate.hpp"
#include "career_1990.hpp"
#include "fq_tyres.hpp"
#include "weekend.hpp"

SWOFTY_SETTING(bool, tyre_class_fix, true);

namespace {

using f1990::at;
namespace t = f1990::tyres;

constexpr const char *kClass1990 = "f1_1990";
constexpr size_t kVisualEntry = 0x10, kMaxVisuals = 11, kFrontendDry = 0x10, kFrontendWet = 0x18;

std::mutex g_lock;
const void *g_career;
bool g_1990;
int64_t g_stat = -1;
std::bitset<64> g_logged;
std::atomic<uint32_t> g_skips;
std::atomic<bool> g_unsafe_logged{false};
std::atomic<bool> g_box_logged{false};

void skip(const char *why, const void *classes, const void *own)
{
    if (g_skips++ < 12)
        swofty::log("[f1-1990] tyre class: lookup %p -> %p kept: %s", classes, own, why);
}

// The rows are tyres.compound_classes.CompoundClass_<i>, which swofty::db::row can't name (it takes the row
// stem from the table name, compound_classe), so they are read by key.
int64_t class_stat()
{
    if (g_stat >= 0)
        return g_stat;
    int64_t count = 0;
    if (swofty::db::int_at("tyres.compound_classes._CompoundClass_instancecount_", &count) != swofty::db::Result::ok)
        return g_stat;
    char key[96];
    for (int64_t i = 0; i < count && i < 64; i++) {
        const char *label = nullptr;
        std::snprintf(key, sizeof key, "tyres.compound_classes.CompoundClass_%lld._metadata_", (long long)i);
        if (swofty::db::string_at(key, &label) != swofty::db::Result::ok || !label || std::strcmp(label, kClass1990))
            continue;
        int64_t stat = -1;
        std::snprintf(key, sizeof key, "tyres.compound_classes.CompoundClass_%lld.stat_id", (long long)i);
        if (swofty::db::int_at(key, &stat) == swofty::db::Result::ok && stat >= 0 && stat < 64)
            g_stat = stat;
        break;
    }
    return g_stat;
}

// The visual compounds of a class (+0x170, {visual compound, physical mask}); 0 when unreadable or more than
// the tyre component holds.
size_t visuals(const uint8_t *cls, const uint8_t *const **begin)
{
    if (!swofty::mem::readable(cls + t::kClassVisuals, 0x10))
        return 0;
    auto *b = at<const uint8_t *>(cls, t::kClassVisuals), *e = at<const uint8_t *>(cls, t::kClassVisuals + 8);
    if (!b || e <= b || size_t(e - b) % kVisualEntry || size_t(e - b) / kVisualEntry > kMaxVisuals ||
        !swofty::mem::readable(b, size_t(e - b)))
        return 0;
    *begin = reinterpret_cast<const uint8_t *const *>(b);
    return size_t(e - b) / kVisualEntry;
}

// The map is indexed by a visual compound's first byte, so a list entry with the same first byte gets the slot.
bool holds(const uint8_t *const *list, size_t n, const uint8_t *visual)
{
    if (!visual || !swofty::mem::readable(visual, 1))
        return false;
    for (size_t i = 0; i < n; i++)
        if (list[i * 2] == visual || (list[i * 2] && swofty::mem::readable(list[i * 2], 1) && *list[i * 2] == *visual))
            return true;
    return false;
}

// Why the class can't take the cars' tyre models, or nullptr when it can.
const char *unsafe(const uint8_t *cls)
{
    const uint8_t *const *list = nullptr;
    size_t n = visuals(cls, &list);
    if (!n)
        return "its visual compound list is unreadable or longer than the tyre component holds";
    if (!swofty::mem::readable(cls + kFrontendDry, 0x10))
        return "its frontend compounds are unreadable";
    auto *dry = at<const uint8_t *>(cls, kFrontendDry), *wet = at<const uint8_t *>(cls, kFrontendWet);
    if (!holds(list, n, dry) || !holds(list, n, wet))
        return "its frontend compound (dry or wet) is not one of its visual compounds, so a showroom car would take "
               "a model slot it doesn't have";
    return nullptr;
}

bool career_1990()
{
    void *career = swofty::career::entity();
    if (career != g_career) {
        g_career = career;
        g_1990 = career && f1990::is_1990_career(career);
        g_logged.reset();
    }
    return g_1990;
}

}

SWOFTY_WRAP(Sess_TyreClass_FromName, void *, void *classes, const void *name, uint8_t check_weekend_format)
{
    void *own = orig(classes, name, check_weekend_format);
    if (!tyre_class_fix || !own || !classes)
        return own;
    std::lock_guard lock(g_lock);
    bool box = false;
    if (!career_1990()) {
        box = f1990::box_list_selected();
        if (!box) {
            skip(swofty::career::entity() ? "the loaded career is not a 1990 one" : "no career loaded", classes, own);
            return own;
        }
    }
    int64_t stat = class_stat();
    auto *c = static_cast<uint8_t *>(classes);
    if (stat < 0 || !swofty::mem::readable(c + 0xf0, 0x10)) {
        skip(stat < 0 ? "no f1_1990 compound class row" : "the class list is unreadable", classes, own);
        return own;
    }
    auto *b = at<uint8_t *>(c, 0xf0), *e = at<uint8_t *>(c, 0xf8);
    auto *target = b + size_t(stat) * t::kClassSize, *from = static_cast<uint8_t *>(own);
    if (!b || target + t::kClassSize > e || from < b || from >= e || from == target ||
        size_t(from - b) % t::kClassSize || !swofty::mem::readable(target, t::kClassSize)) {
        skip(from == target ? "already f1_1990"
             : !b || target + t::kClassSize > e ? "the f1_1990 class is past the class list"
                                                 : "the result is not an entry of the class list",
             classes, own);
        return own;
    }
    size_t index = size_t(from - b) / t::kClassSize;
    if (const char *why = unsafe(target)) {
        if (!g_unsafe_logged.exchange(true))
            swofty::log("[f1-1990] tyre class: f1_1990 (class %lld) kept off every car: %s", static_cast<long long>(stat),
                        why);
        return own;
    }
    bool same = t::same_physical(from, target);
    if (index < g_logged.size() && !g_logged.test(index)) {
        g_logged.set(index);
        swofty::log(same ? "[f1-1990] tyre class: 1990 career cars on compound class %zu take f1_1990 (class %lld)"
                         : "[f1-1990] tyre class: 1990 career cars on compound class %zu keep it: other physical "
                           "compounds than f1_1990 (class %lld)",
                    index, static_cast<long long>(stat));
    }
    if (box && same && !g_box_logged.exchange(true))
        swofty::log("[f1-1990] tyre class: F1 1990 box session (list %s): cars on compound class %zu take f1_1990 "
                    "(class %lld)",
                    "f1_1990_gp or f1_1990_tt", index, static_cast<long long>(stat));
    return same ? target : own;
}

namespace {

void on_career(const SwoftyEvent *, void *)
{
    std::lock_guard lock(g_lock);
    g_career = nullptr;
    g_1990 = false;
    g_logged.reset();
}

}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] tyre class loaded: %s", tyre_class_fix ? "on" : "off (tyre_class_fix)");
    uint32_t token = 0;
    swofty::events::subscribe(SWOFTY_EVENT_CAREER_LOADED, on_career, nullptr, &token);
    swofty::events::subscribe(SWOFTY_EVENT_CAREER_UNLOADED, on_career, nullptr, &token);
}

SWOFTY_ADDON()
