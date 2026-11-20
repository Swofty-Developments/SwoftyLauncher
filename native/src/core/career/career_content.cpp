#include "core/career/content_state.hpp"
#include "core/career/resume_list.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstring>
#include <mutex>
#include <set>
#include <string>

#include "core/career/career_content_logic.hpp"
#include "core/registry/older.hpp"

// Extra career contents (swofty/career_content.hpp). An addon registers a content from its init;
// the core then owns everything the stock code would refuse for it:
//
// Tile and Addons row: content_tiles.cpp. Router, gate and slot predicate: content_hooks.cpp. The
// addon table (SwoftyCoreCareerContent): content_api.cpp.
//
// The tile, gate and predicate hooks are one core mixin set, offered with the router at the first
// mixin commit after a content was registered, so a game without registered contents runs none of
// them, and a build where one of them, the router or a binding the handlers call does not resolve
// turns every content off with a log line.

namespace career::content {

Slot slots[content::MAX_CONTENTS];
Bound b{};
constinit ContentHooks hooks;
constinit RouteHooks route;
std::mutex pressed_lock;
std::string pressed_group;
bool pressed_any;

namespace {

constexpr const char *MIXIN = "swofty_core.dll (career contents)";
constexpr const char *ROUTE_MIXIN = "swofty_core.dll (career tile route)";

std::set<std::string> records_tried;
std::atomic<size_t> count{0};
std::atomic<bool> route_wanted{false};

bool db_text(const std::string &key, std::string &out, void *)
{
    const SwoftyDb *db = runtime::db_table();
    SwoftyDbValue v{};
    if (db->get(key.c_str(), &v) != SWOFTY_ASSET_OK || !v.s)
        return false;
    out = v.s;
    return true;
}

bool db_integer(const std::string &key, int64_t &out, void *)
{
    return runtime::db_table()->int_at(key.c_str(), &out) == SWOFTY_ASSET_OK;
}

bool db_list_stat(const std::string &label, int64_t &out, void *)
{
    const SwoftyDb *db = runtime::db_table();
    SwoftyDbCursor c{};
    SwoftyDbValue v{};
    if (db->row("entrants.entrant_lists", label.c_str(), &c) != SWOFTY_ASSET_OK || db->cell(&c, "stat_id", &v) != SWOFTY_ASSET_OK)
        return false;
    out = v.i;
    return true;
}

}

int64_t mode_stat_of(const std::string &save)
{
    if (save.empty() || !runtime::db_table()->available())
        return -1;
    content::Db db{db_text, db_integer, db_list_stat, nullptr};
    return content::mode_stat_id(db, save);
}

void resolve(Slot &s, bool now, bool building)
{
    if (s.resolved || s.state != SWOFTY_CAREER_CONTENT_LIVE || !runtime::db_table()->available())
        return;
    std::lock_guard<std::mutex> g(hooks.lock);
    uint64_t t = GetTickCount64();
    if (s.resolved || (!now && !s.retry.due(t)))
        return;
    content::Db db{db_text, db_integer, db_list_stat, nullptr};
    content::Requirements r = content::requirements(s.desc, db);
    s.mode = r.mode;
    s.group = r.group;
    std::string why = r.why;
    uintptr_t gm = game_manager_slot();
    content::Loaded l;
    if (r.ok && gm) {
        l = loaded_lists(gm, r.group);
        if (!l.known && !building) {
            s.retry.next = t + content::RETRY_MS;
            return;
        }
        why = content::loaded_why(s.desc, r.group, l);
    }
    if (!why.empty()) {
        s.why = why;
        if (s.retry.failed(t, why))
            swofty::log("[career] content '%s' (%s) not ready: %s; no tile and its mode keeps the stock gate until it is, "
                        "checked again every %llu ms",
                        s.desc.id.c_str(), s.owner.c_str(), why.c_str(), (unsigned long long)content::RETRY_MS);
        return;
    }
    s.why.clear();
    s.resolved = true;
    if (!r.warn.empty())
        swofty::log("[career] content '%s': %s", s.desc.id.c_str(), r.warn.c_str());
    std::string slots_seen = !gm       ? "; loaded list slots not checked: g_Game_Manager does not resolve"
                             : l.known ? " of " + std::to_string(l.slots) + " loaded list slots"
                                       : "; loaded list slots not checked: the entrants system +0x560 holds no list "
                                         "slots on this build";
    swofty::log("[career] content '%s' ready%s: mode %s statId %lld, data group %s (entrant list stat_id %lld%s), suffix "
                "%s (F1CAREER_%s%s%s)",
                s.desc.id.c_str(), s.retry.tries ? (" after " + std::to_string(s.retry.tries + 1) + " checks").c_str() : "",
                s.desc.mode.c_str(), (long long)r.mode, s.desc.data_group.c_str(), (long long)r.group, slots_seen.c_str(),
                s.desc.suffix.c_str(), s.desc.suffix.c_str(), r.f2 ? ", F2CAREER" : "", r.icons ? ", ICONS" : "");
}

namespace {

namespace older = swofty_core::registry::older;

older::ContentDesc to_older(const content::Desc &d)
{
    return {d.id,          d.title, d.description, d.data_group, d.suffix, d.mode,
            d.entitlement, d.model, d.car_group,   d.badge,      d.careers};
}

void from_records()
{
    auto &st = runtime::older_store();
    for (auto &id : older::in_load_order(st, older::CONTENT)) {
        std::string addon = older::addon_of(id);
        if (addon.empty() || addon == "game" || addon == "swofty.core")
            continue;
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            bool made = false;
            for (size_t i = 0; i < used(); i++)
                made = made || slots[i].record == id;
            if (made || !records_tried.insert(id).second)
                continue;
        }
        if (!runtime::enabled("career", nullptr)) {
            swofty::log("[career] career.content record %s from %s makes no tile: the career feature is off", id.c_str(),
                        addon.c_str());
            continue;
        }
        swofty::json::Value rec = older::effective(st, older::CONTENT, id);
        std::string mode_id = older::content_mode_id(id, rec);
        swofty::json::Value mode = mode_id.empty() ? swofty::json::Value() : older::effective(st, older::MODE, mode_id);
        older::ContentDesc d;
        std::string why;
        if (!older::content_from(id, rec, mode, d, why)) {
            swofty::log("[career] career.content record %s from %s makes no tile: %s", id.c_str(), addon.c_str(), why.c_str());
            continue;
        }
        content::Desc desc{d.id,          d.title, d.description, d.data_group, d.suffix, d.mode,
                           d.entitlement, d.model, d.car_group,   d.badge,      d.careers};
        add_slot(desc, addon, addon, &id);
    }
}

}

int add_slot(const content::Desc &desc, const std::string &who, const std::string &addon, const std::string *record)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    size_t n = used();
    std::string own = addon + "/" + desc.id;
    size_t at = n;
    if (!record)
        for (size_t i = 0; i < n; i++)
            if (slots[i].from_record && slots[i].record == own && !slots[i].vt &&
                (!hooks.gate.offered() || slots[i].desc.model == desc.model))
                at = i;
    std::string why = content::check(desc, [&](const char *field, const std::string &v) {
        for (size_t i = 0; i < n; i++) {
            const content::Desc &o = slots[i].desc;
            if (i != at && ((!strcmp(field, "id") && o.id == v) || (!strcmp(field, "data_group") && o.data_group == v) ||
                            (!strcmp(field, "suffix") && o.suffix == v)))
                return true;
        }
        return false;
    });
    if (why.empty() && at == n && n >= content::MAX_CONTENTS)
        why = "the core holds at most 8 contents";
    if (!why.empty()) {
        if (record)
            swofty::log("[career] career.content record %s from %s makes no tile: %s", record->c_str(), who.c_str(), why.c_str());
        else
            swofty::log("[career] content '%s' from %s refused: %s", desc.id.c_str(), who.c_str(), why.c_str());
        return SWOFTY_ASSET_INVALID;
    }
    Slot &s = slots[at];
    bool took_over = at < n;
    s.desc = desc;
    s.given = desc;
    s.owner = who;
    s.from_record = record != nullptr;
    if (record)
        s.record = *record;
    else if (older::put(runtime::older_store(), addon, older::CONTENT, desc.id, older::content_record(to_older(desc)),
                        "career::content::add") == SWOFTY_ASSET_OK ||
             took_over)
        s.record = own;
    else
        s.record.clear();
    if (took_over) {
        s.resolved = false;
        s.retry = {};
        s.why.clear();
    }
    uint32_t state = hooks.off()    ? SWOFTY_CAREER_CONTENT_OFF
                     : hooks.live() ? SWOFTY_CAREER_CONTENT_LIVE
                                    : SWOFTY_CAREER_CONTENT_QUEUED;
    if (hooks.off())
        s.why = hooks.why();
    s.state = state;
    if (!took_over)
        count.store(n + 1, std::memory_order_release);
    std::string from = record      ? who + " (career.content record " + *record + ", no call)"
                       : took_over ? who + " (the call replaces the tile its career.content record " + own + " made)"
                                   : who;
    swofty::log("[career] content '%s' from %s registered (tile %zu after the stock ones, %s model, data group %s, suffix "
                "%s, mode %s): %s",
                desc.id.c_str(), from.c_str(), at + 1, desc.model == content::MODEL_2025 ? "2025" : "2026",
                desc.data_group.c_str(), desc.suffix.c_str(), desc.mode.c_str(),
                state == SWOFTY_CAREER_CONTENT_LIVE  ? "live (hooks already installed)"
                : state == SWOFTY_CAREER_CONTENT_OFF ? ("off: " + hooks.why()).c_str()
                                                     : "queued for the next mixin commit");
    return hooks.off() ? SWOFTY_ASSET_UNAVAILABLE : SWOFTY_ASSET_OK;
}

void turn_off(Slot &s, const std::string &why)
{
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        s.why = why;
    }
    s.state = SWOFTY_CAREER_CONTENT_OFF;
    swofty::log("[career] content '%s' (%s) is off: %s", s.desc.id.c_str(), s.owner.c_str(), why.c_str());
}

size_t used() { return count.load(std::memory_order_acquire); }

void resolve_all(bool now, bool building)
{
    for (size_t i = 0; i < used(); i++)
        resolve(slots[i], now, building);
}

bool ready(const Slot &s) { return s.state == SWOFTY_CAREER_CONTENT_LIVE && s.resolved; }

Slot *by_vtable(const void *item)
{
    void **vt = item && mem::readable(item, 8) ? *static_cast<void **const *>(item) : nullptr;
    for (size_t i = 0; vt && i < used(); i++)
        if (slots[i].vt == vt)
            return &slots[i];
    return nullptr;
}

void shown_from_records()
{
    auto &st = runtime::older_store();
    for (size_t i = 0; i < used(); i++) {
        Slot &s = slots[i];
        if (s.record.empty())
            continue;
        older::ContentDesc d = to_older(s.given);
        swofty::json::Value rec = older::effective(st, older::CONTENT, s.record);
        if (s.from_record) {
            std::string why, mode_id = older::content_mode_id(s.record, rec);
            older::ContentDesc fresh;
            if (older::content_from(s.record, rec,
                                    mode_id.empty() ? swofty::json::Value() : older::effective(st, older::MODE, mode_id), fresh,
                                    why))
                d = fresh;
        } else {
            older::content_shown(rec, d);
        }
        s.desc.title = d.title;
        s.desc.description = d.description;
        s.desc.badge = d.badge;
        s.desc.car_group = d.car_group;
        s.desc.entitlement = d.entitlement;
        s.desc.careers = d.careers;
    }
}

}

using namespace career::content;

namespace runtime {

std::string career_content_owner(const std::string &data_group, std::string *save_name)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    for (size_t i = 0; i < used(); i++)
        if (slots[i].desc.data_group == data_group) {
            if (save_name)
                *save_name = slots[i].desc.mode;
            return slots[i].owner;
        }
    return {};
}

static std::vector<CoreMixins> career_content_mixins()
{
    std::vector<CoreMixins> out;
    from_records();
    if (hooks.gate.offered() || !(used() || career::resume_list::wanted()) || hooks.off() || !code_ready())
        return out;
    hooks.gate.offer();
    std::string why;
    if (route.gate.settled() && !route.live())
        why = "the career tile route hook did not install (see the [core] lines for " + std::string(ROUTE_MIXIN) + ")";
    if (!why.empty() || !bind_all(why)) {
        hooks.turn_off(why);
        for (size_t i = 0; i < used(); i++)
            turn_off(slots[i], why);
        return out;
    }
    out.push_back({MIXIN,
                   {{"site_FeTile_CareerContentEnd", SWOFTY_MIXIN_SITE, 100, (void *)&site_entry, nullptr},
                    {"Game_Career_IsDriverCareerMode", SWOFTY_MIXIN_WRAP, 100, (void *)&gate_entry, hooks.driver_gate.slot()},
                    {"vt_Game_DriverCareerSlotPredicate", SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 2), 100,
                     (void *)&predicate_entry, hooks.slot_predicate.slot()},
                    {"vt_Game_CareerDLCSelectViewModel", SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 10), 100,
                     (void *)&command_entry, hooks.select_command.slot()}}});
    tile_route_want();
    swofty::log("[career] contents: %zu registered; offering the career content hooks", used());
    return out;
}

void tile_route_want() { route_wanted = true; }

static std::vector<CoreMixins> tile_route_mixins()
{
    std::vector<CoreMixins> out;
    if (!route_wanted || route.gate.offered())
        return out;
    route.gate.offer();
    out.push_back(
        {ROUTE_MIXIN, {{"FeTile_Route_CareerItem", SWOFTY_MIXIN_WRAP, 100, (void *)&route_entry, route.route_item.slot()}}});
    return out;
}

static void tile_route_commit()
{
    if (!route.gate.offered() || route.gate.settled())
        return;
    if (core_mixins_active(ROUTE_MIXIN))
        route.set_live();
    route.gate.settle();
    swofty::log("[career] career tile route: %s", route.live() ? "armed" : "off");
}

static loader::FunctionSet tile_route_core_set{loader::CoreSetId::tile_route, tile_route_mixins, tile_route_commit};

TileRoute tile_route_state()
{
    return route.live()                                                     ? TileRoute::live
           : route.gate.settled() || (route_wanted && core_mixins_closed()) ? TileRoute::off
                                                                            : TileRoute::pending;
}

static void career_content_commit()
{
    if (!route.gate.settled() || !hooks.gate.settle())
        return;
    if (core_mixins_active(MIXIN) && route.live()) {
        hooks.set_live();
        for (size_t i = 0; i < used(); i++)
            if (slots[i].state == SWOFTY_CAREER_CONTENT_QUEUED)
                slots[i].state = SWOFTY_CAREER_CONTENT_LIVE;
        swofty::log("[career] contents: hooks installed; %zu content(s) live, their data is checked once the database "
                    "has loaded",
                    used());
        return;
    }
    hooks.turn_off("the career content hooks did not install (see the [core] lines for " +
                   std::string(route.live() ? MIXIN : ROUTE_MIXIN) + ")");
    for (size_t i = 0; i < used(); i++)
        if (slots[i].state != SWOFTY_CAREER_CONTENT_OFF)
            turn_off(slots[i], hooks.why());
}

static loader::FunctionSet career_content_core_set{loader::CoreSetId::career_content, career_content_mixins,
                                                   career_content_commit};

void career_content_tick()
{
    if (hooks.live() && used())
        resolve_all(false);
}

}
