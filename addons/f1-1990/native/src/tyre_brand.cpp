// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the player's tyre supplier (its own DLL, so a binding it can't resolve drops only this feature).
//
// A wheel's physical compound is named "<TyrePhysicsGroup>_<physics code><f|r>" (Car_Tyre_SetWearFactor
// 0x142ea2300, format 0x1457ef36f), the group read from the car's live Chassis system (CarLive+0x558) through
// its TyrePhysicsGroup string object (+0x118: +0x08 capacity, +0x10 buffer). The Goodyear teams' handling files
// carry F1_1990 and the Pirelli teams' F1_1990P. In a 1990 session the player's weekend Tyre Allocation choice
// (the allocation's preset type, +0x04: 0 Harder, 1 Balanced, 2 Softer) picks the supplier of the player's team:
// Harder is Goodyear, Softer is Pirelli and Balanced keeps the team's 1990 supplier. At every 1990 session start
// both cars of the player's team get the chosen group written into their own Chassis string (each CarLive is
// spawned per session from the handling file) and their fitted compounds applied again on the game thread. A
// change of the choice during a session rewrites the strings only; the next tyre fit applies it. Every other car
// keeps its handling file's group. While a 1990 career is loaded, data/tyre-brand-strings.json renames the three
// choices (lng_harder Goodyear, lng_softer Pirelli, lng_balanced 1990 Supplier) and their suggestion lines through
// the core's runtime string overrides (API v8).

#include <cstdio>
#include <cstring>
#include <mutex>
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
#include "tyre_brand.hpp"
#include "tyre_cars.hpp"
#include "box_gate.hpp"
#include "weekend.hpp"

SWOFTY_SETTING(bool, player_tyre_supplier, true);

namespace {

using f1990::at;
namespace b = f1990::brand;
namespace c = f1990::cars;

constexpr size_t kCarLive = 0x58;
constexpr size_t kChassis = 0x558;
constexpr uint8_t kHumanFlag = 0x10;

struct Original {
    char *buf;
    char text[16];
};

std::mutex g_lock;
void *g_component;
uint32_t g_session;
std::vector<Original> g_originals;

void *call_getter(void *obj, size_t slot)
{
    auto **vt = *static_cast<void ***>(obj);
    return vt && vt[slot] ? reinterpret_cast<void *(*)(void *)>(vt[slot])(obj) : nullptr;
}

uint8_t *allocation_of(void *component, uint32_t id)
{
    auto *base = static_cast<uint8_t *>(component);
    if (!swofty::mem::readable(base + 0x2b0, 0x10))
        return nullptr;
    auto *p = at<uint8_t *>(base, 0x2b0), *e = at<uint8_t *>(base, 0x2b8);
    if (!p || e <= p || size_t(e - p) % 0x10 || !swofty::mem::readable(p, size_t(e - p)))
        return nullptr;
    for (; p < e; p += 0x10) {
        auto *alloc = at<uint8_t *>(p, 0x08);
        if (!alloc || !swofty::mem::readable(alloc, 0x10))
            continue;
        uint32_t a = at<uint32_t>(alloc, 0);
        if (uint8_t(a) == uint8_t(id) && uint16_t(a >> 16) == uint16_t(id >> 16))
            return alloc;
    }
    return nullptr;
}

char *group_buffer(uint8_t *tyres, size_t *capacity)
{
    if (!swofty::mem::readable(tyres + 0x20, 8))
        return nullptr;
    auto *record = at<uint8_t *>(tyres, 0x20);
    if (!record || !swofty::mem::readable(record + kCarLive, 8))
        return nullptr;
    auto *car = at<uint8_t *>(record, kCarLive);
    if (!car || !swofty::mem::readable(car + kChassis, 8))
        return nullptr;
    auto *chassis = at<uint8_t *>(car, kChassis);
    if (!chassis || !swofty::mem::readable(chassis + b::kChassisGroup, 0x18))
        return nullptr;
    *capacity = at<size_t>(chassis, b::kChassisGroup + b::kStringCapacity);
    auto *buf = at<char *>(chassis, b::kChassisGroup + b::kStringBuffer);
    if (!buf || *capacity < sizeof b::kPirelliGroup || *capacity > 0x1000 || swofty::mem::in_image(uintptr_t(buf)) ||
        !swofty::mem::readable(buf, *capacity))
        return nullptr;
    return buf;
}

const char *original_of(char *buf)
{
    for (auto &o : g_originals)
        if (o.buf == buf)
            return o.text;
    return nullptr;
}

void remember(char *buf)
{
    if (original_of(buf))
        return;
    Original o{buf, {}};
    std::snprintf(o.text, sizeof o.text, "%s", buf);
    g_originals.push_back(o);
}

void apply(void *component, uint32_t session, const char *why, bool refit)
{
    c::Entrants entrants(component);
    const uint8_t *human = nullptr;
    for (const uint8_t *r = entrants.b; r < entrants.e && !human; r += c::kEntrantSize)
        if ((at<uint8_t>(r, 0x4b) & kHumanFlag) && at<uint8_t>(r, 0x82) != 0xff)
            human = r;
    if (!human) {
        swofty::log("[f1-1990] tyre brand: session %u (%s): no player car", session, why);
        return;
    }
    uint8_t *alloc = allocation_of(component, entrants.id(human));
    if (!alloc) {
        swofty::log("[f1-1990] tyre brand: session %u (%s): the player has no weekend allocation", session, why);
        return;
    }
    uint32_t preset = at<uint32_t>(alloc, 4);
    b::Brand brand = b::for_preset(preset);
    uint16_t team = at<uint16_t>(human, 0x5a);
    void *car_list = swofty::mem::readable(static_cast<uint8_t *>(component) + 0x2d8, 8) ? at<void *>(component, 0x2d8)
                                                                                          : nullptr;
    int cars = 0, switched = 0, refitted = 0, skipped = 0;
    for (const uint8_t *r = entrants.b; r < entrants.e; r += c::kEntrantSize) {
        uint8_t v = at<uint8_t>(r, 0x82);
        if (v == 0xff || at<uint16_t>(r, 0x5a) != team)
            continue;
        cars++;
        uint8_t *tyres = c::tyre_component(car_list, v);
        size_t capacity = 0;
        char *buf = tyres ? group_buffer(tyres, &capacity) : nullptr;
        if (!buf || !b::is_1990_group(buf, capacity)) {
            skipped++;
            continue;
        }
        remember(buf);
        const char *wanted = brand == b::Brand::team ? original_of(buf) : b::group(brand);
        int n = b::write_group(buf, capacity, wanted);
        if (n < 0) {
            skipped++;
            continue;
        }
        if (!n)
            continue;
        switched++;
        if (refit) {
            swofty::game::Car_Tyre_SetWearFactor(tyres, true, 0x101);
            refitted++;
        }
    }
    swofty::log("[f1-1990] tyre brand: session %u (%s): player's team %04x on %s (allocation choice %u): %d car(s), "
                "%d switched, %d refitted, %d skipped",
                session, why, team, b::name(brand), preset, cars, switched, refitted, skipped);
}

#if SWOFTY_API_VERSION >= 8
constexpr char kStringsFile[] = "data/tyre-brand-strings.json";

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
        swofty::log("[f1-1990] tyre brand: read %s: %s", kStringsFile, swofty::storage::name(r));
        return;
    }
    if (!f1990::parse_string_leaves(json, leaves, &error_at)) {
        swofty::log("[f1-1990] tyre brand: %s is not an object of strings (byte %zu)", kStringsFile, error_at);
        return;
    }
    for (auto &l : leaves)
        if (l.path.size() == 3 && l.path[0] == "strings")
            g_strings.push_back({l.path[1], l.path[2], l.text});
}

void choice_names(bool on)
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
    swofty::log("[f1-1990] tyre brand: %s %zu allocation choice string(s) (%zu failed)", on ? "set" : "cleared",
                g_strings.size(), failed);
}

void on_career(const SwoftyEvent *ev, void *)
{
    bool yes = false;
    if (player_tyre_supplier && !std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED)) {
        void *career = swofty::career::entity();
        yes = career && f1990::is_1990_career(career);
    }
    choice_names(yes);
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] tyre brand: subscribe %s failed; the allocation choices keep their stock names", event);
}
#endif

}

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    if (!component)
        return;
    void *provider = at<void *>(component, 0x270);
    void *session = provider ? call_getter(provider, 0xa8 / 8) : nullptr;
    void *weekend = provider ? call_getter(provider, 0x98 / 8) : nullptr;
    bool is_1990 = session && weekend && (f1990::is_1990_weekend(weekend) || f1990::box_list_selected());
#if SWOFTY_API_VERSION >= 8
    if (player_tyre_supplier) {
        void *career = swofty::career::entity();
        std::lock_guard lock(g_lock);
        choice_names(is_1990 || (career && f1990::is_1990_career(career)));
    }
#endif
    std::lock_guard lock(g_lock);
    g_originals.clear();
    g_component = is_1990 ? component : nullptr;
    g_session = session ? at<uint32_t>(session, 0x08) : 0;
    if (is_1990 && player_tyre_supplier)
        apply(component, g_session, "session start", true);
}

SWOFTY_WRAP(Game_TyreAllocationComponent_SetPresetChoiceForHumanDriver, void, void *component, uint32_t preset,
            int local_player)
{
    orig(component, preset, local_player);
    std::lock_guard lock(g_lock);
    if (player_tyre_supplier && component && component == g_component)
        apply(component, g_session, "allocation choice", false);
}

SWOFTY_ON_LOAD
{
#if SWOFTY_API_VERSION >= 8
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
#endif
    swofty::log("[f1-1990] tyre brand loaded: the player's team %s",
                player_tyre_supplier ? "runs the supplier of its weekend Tyre Allocation choice (Harder Goodyear, "
                                       "Softer Pirelli, Balanced its 1990 supplier)"
                                     : "keeps its 1990 supplier (player_tyre_supplier off)");
}

SWOFTY_ADDON()
