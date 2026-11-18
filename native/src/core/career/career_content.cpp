#include "core/career/content_state.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"
#include "core/runtime/resolve.hpp"

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
// Tile and Addons row: content_tiles.cpp.
//
// Router (FeTile_Route_CareerItem, wrapped): every career tile press ends in it; the wrap records
// the pressed tile's data group (pressed()), fires career.content.pressed and, after the game's
// router, hands the route to the career wizard library (career_wizard.cpp). The wrap is a core
// mixin set of its own, shared by both libraries and offered at the first mixin commit after either
// of them needs it, so a content and a wizard entry registered at different stages never wrap the
// router twice.
//
// Gate (Game_Career_IsDriverCareerMode, wrapped): 1 only for the stock 2025 and 2026 driver-career
// modes, so a mode championship whose statId (+0x2a8) is a content's mode (game_modes statId of the
// row that saves to desc.mode) is passed.
//
// Slot predicate (vt_Game_DriverCareerSlotPredicate slot 2): asked for a content's data group, a
// stock false for that content's mode championship becomes the predicate's own data group check
// (career_content_logic.hpp). Questions for any other data group keep the game's answer.
//
// The tile, gate and predicate hooks are one core mixin set, offered with the router at the first
// mixin commit after a content was registered, so a game without registered contents runs none of
// them, and a build where one of them, the router or a binding the handlers call does not resolve
// turns every content off with a log line.

namespace career_content {

Slot slots[content::MAX_CONTENTS];
Bound b{};

namespace {

constexpr const char *MIXIN = "swofty_core.dll (career contents)";
constexpr const char *ROUTE_MIXIN = "swofty_core.dll (career tile route)";

using Gate = bool(const void *);
using Predicate = bool(void *, void *);
using RegistryGet = void *(void *, const void *, int);
using GroupIndex = uint8_t(void *);

std::mutex lock;
std::set<std::string> records_tried;
std::atomic<size_t> count{0};
runtime::loader::MixinGate mixin_gate, route_mixin_gate;
std::atomic<bool> live{false}, off{false};
std::atomic<bool> route_wanted{false}, route_live{false};
std::string off_why;
Route *route_orig;
Gate *gate_orig;
Predicate *predicate_orig;
std::mutex pressed_lock;
std::string pressed_group;
bool pressed_any;

std::string text(const char *s)
{
    return s ? s : "";
}

int copy_out(const std::string &s, char *out, size_t cap)
{
    if (!out || !cap)
        return SWOFTY_ASSET_INVALID;
    size_t n = s.size() < cap - 1 ? s.size() : cap - 1;
    std::memcpy(out, s.data(), n);
    out[n] = 0;
    return n == s.size() ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

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
    if (db->row("entrants.entrant_lists", label.c_str(), &c) != SWOFTY_ASSET_OK ||
        db->cell(&c, "stat_id", &v) != SWOFTY_ASSET_OK)
        return false;
    out = v.i;
    return true;
}

uintptr_t game_manager_slot();
content::Loaded loaded_lists(uintptr_t slot, int64_t id);

void resolve(Slot &s, bool now, bool building = false)
{
    if (s.resolved || s.state != SWOFTY_CAREER_CONTENT_LIVE || !runtime::db_table()->available())
        return;
    std::lock_guard<std::mutex> g(lock);
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
    std::string slots_seen = !gm      ? "; loaded list slots not checked: g_Game_Manager does not resolve"
                             : l.known ? " of " + std::to_string(l.slots) + " loaded list slots"
                                       : "; loaded list slots not checked: the entrants system +0x560 holds no list "
                                         "slots on this build";
    swofty::log("[career] content '%s' ready%s: mode %s statId %lld, data group %s (entrant list stat_id %lld%s), suffix "
                "%s (F1CAREER_%s%s%s)",
                s.desc.id.c_str(), s.retry.tries ? (" after " + std::to_string(s.retry.tries + 1) + " checks").c_str() : "",
                s.desc.mode.c_str(), (long long)r.mode, s.desc.data_group.c_str(), (long long)r.group,
                slots_seen.c_str(), s.desc.suffix.c_str(), s.desc.suffix.c_str(), r.f2 ? ", F2CAREER" : "",
                r.icons ? ", ICONS" : "");
}

Slot *by_mode(const void *championship)
{
    if (!championship || !mem::readable(static_cast<const uint8_t *>(championship) + content::MODE_STAT_ID, 4))
        return nullptr;
    uint32_t mode = content::at<uint32_t>(championship, content::MODE_STAT_ID);
    for (size_t i = 0; i < used(); i++)
        if (ready(slots[i]) && slots[i].mode == int64_t(mode))
            return &slots[i];
    return nullptr;
}

void *setup_component(void *championship, void *)
{
    auto *reg = static_cast<uint8_t *>(championship) + content::CHAMP_REGISTRY;
    void **vt = mem::readable(reg, 8) ? *reinterpret_cast<void ***>(reg) : nullptr;
    if (!vt || !mem::in_image(vt) || !mem::readable(vt + 2, 8) || !mem::in_image(vt[2]))
        return nullptr;
    void *c = reinterpret_cast<RegistryGet *>(vt[2])(reg, reinterpret_cast<const void *>(b.setup_desc), 1);
    return c && mem::object_like(c) ? c : nullptr;
}

int group_index(void *component, void *)
{
    void **vt = *static_cast<void ***>(component);
    auto *slot = reinterpret_cast<uint8_t *>(vt) + content::SETUP_INDEX_SLOT;
    void *fn = mem::readable(slot, 8) ? *reinterpret_cast<void **>(slot) : nullptr;
    return fn && mem::in_image(fn) ? reinterpret_cast<GroupIndex *>(fn)(component) : -1;
}

bool readable(const void *p, size_t n, void *)
{
    return mem::readable(p, n);
}

const content::Access ACCESS{setup_component, group_index, readable, nullptr};

uintptr_t game_manager_slot()
{
    static const uintptr_t slot = swofty::api->resolve("g_Game_Manager");
    return slot;
}

content::Loaded loaded_lists(uintptr_t slot, int64_t id)
{
    uintptr_t gm = 0, entrants = 0;
    if (!mem::read(slot, gm) || !gm || !mem::read(gm + runtime::manager_entrants_system(), entrants) || !entrants)
        return {};
    return content::loaded_list(reinterpret_cast<const void *>(entrants), id, ACCESS);
}

Action *route_entry(HomeItem *item, Action *out)
{
    Slot *s = by_vtable(item);
    const char *group = item && mem::readable(reinterpret_cast<uint8_t *>(item) + content::ITEM_DATA_GROUP, 8)
                            ? nestr(content::at<const NeStr *>(item, content::ITEM_DATA_GROUP))
                            : nullptr;
    std::string g = group ? group : "";
    {
        std::lock_guard<std::mutex> l(pressed_lock);
        pressed_group = g;
        pressed_any = true;
    }
    if (used())
        runtime::events_fire(SWOFTY_EVENT_CAREER_CONTENT_PRESSED, s ? int64_t(s - slots) : -1, 0, g.c_str());
    Action *r = route_orig(item, out);
    runtime::wizard_tile_routed(g, out);
    return r;
}

bool gate_entry(const void *championship)
{
    Slot *s = used() ? by_mode(championship) : nullptr;
    if (!s)
        return gate_orig(championship);
    if (s->gates++ < LOG_FIRST)
        swofty::log("[career] content '%s': its mode (statId %lld) passes the driver-career gate", s->desc.id.c_str(),
                    (long long)s->mode.load());
    return true;
}

bool predicate_entry(void *self, void *championship)
{
    bool stock = predicate_orig(self, championship);
    if (stock || !championship || !used())
        return stock;
    Slot *s = by_mode(championship);
    auto *holder = s && mem::readable(static_cast<uint8_t *>(self) + 8, 8) ? content::at<const uint8_t *>(self, 0x08)
                                                                           : nullptr;
    const char *want = holder && mem::readable(holder, 8) ? content::at<const char *>(holder, 0x00) : nullptr;
    if (!want || !mem::readable(want, 1) || s->desc.data_group != want)
        return false;
    bool ok = content::data_group_is(championship, want, ACCESS);
    if (s->slots++ < LOG_FIRST) {
        content::GroupSeen seen = content::data_group(championship, ACCESS);
        swofty::log("[career] content '%s': save-slot predicate said no for its mode; data group %d of %zu is \"%s\", "
                    "wanted \"%s\": %s",
                    s->desc.id.c_str(), seen.index, seen.count, seen.name ? seen.name : "(none)", want,
                    ok ? "match" : "no match");
    }
    return ok;
}

bool bind_all(std::string &why)
{
    std::string missing;
    bool y25 = false, y26 = false;
    for (size_t i = 0; i < used(); i++)
        (slots[i].desc.model == content::MODEL_2025 ? y25 : y26) = true;
    b.site = runtime::resolve_or_note("site_FeTile_CareerContentEnd", missing);
    b.route = runtime::resolve_or_note("FeTile_Route_CareerItem", missing);
    b.gate = runtime::resolve_or_note("Game_Career_IsDriverCareerMode", missing);
    b.predicate_vt = runtime::resolve_or_note("vt_Game_DriverCareerSlotPredicate", missing);
    b.select_vt = runtime::resolve_or_note("vt_Game_CareerDLCSelectViewModel", missing);
    b.add_tile = runtime::resolve_or_note("FeHome_Add_CareerTile", missing);
    b.construct = runtime::resolve_or_note("FeTile_Construct_CareerItem", missing);
    b.populate_2026 = y26 ? runtime::resolve_or_note("FeTile_CareerDlcItem_Populate", missing) : 0;
    b.populate_2025 = y25 ? runtime::resolve_or_note("FeTile_CareerDefaultItem_Populate", missing) : 0;
    b.customisation = runtime::resolve_or_note("g_FeTile_CustomisationSystem", missing);
    b.selected = runtime::resolve_or_note("g_FeTile_SelectedCareerContent", missing);
    b.setup_desc = runtime::resolve_or_note("k_Career_CareerSetupComponentDesc", missing);
    b.make_str = runtime::resolve_or_note("Ne_Make_StrFromCStr", missing);
    b.set_string = runtime::resolve_or_note("Vm_Set_String", missing);
    b.vm_registry = runtime::resolve_or_note("g_Vm_Registry", missing);
    b.list_child = swofty::api->resolve("Vm_GetOrCreate_ListChild");
    b.set_bool = swofty::api->resolve("Vm_Set_Bool");
    if (missing.empty())
        return true;
    why = "needs " + missing + ", which this build does not resolve";
    return false;
}

std::string caller(const void *from)
{
    std::string dll, id = settings_addon(from, dll);
    return id.empty() ? (dll.empty() ? std::string("unknown module") : dll) : id;
}

namespace older = swofty_core::registry::older;

older::ContentDesc to_older(const content::Desc &d)
{
    return {d.id, d.title, d.description, d.data_group, d.suffix, d.mode, d.entitlement, d.model, d.car_group, d.badge, d.careers};
}

int api_available()
{
    return runtime::enabled("career", nullptr) && !off.load();
}

int add_slot(const content::Desc &desc, const std::string &who, const std::string &addon, const std::string *record)
{
    std::lock_guard<std::mutex> g(lock);
    size_t n = used();
    std::string own = addon + "/" + desc.id;
    size_t at = n;
    if (!record)
        for (size_t i = 0; i < n; i++)
            if (slots[i].from_record && slots[i].record == own && !slots[i].vt &&
                (!mixin_gate.offered() || slots[i].desc.model == desc.model))
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
            swofty::log("[career] career.content record %s from %s makes no tile: %s", record->c_str(), who.c_str(),
                        why.c_str());
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
                        "career::content::add") == SWOFTY_ASSET_OK || took_over)
        s.record = own;
    else
        s.record.clear();
    if (took_over) {
        s.resolved = false;
        s.retry = {};
        s.why.clear();
    }
    uint32_t state = off ? SWOFTY_CAREER_CONTENT_OFF : live ? SWOFTY_CAREER_CONTENT_LIVE : SWOFTY_CAREER_CONTENT_QUEUED;
    if (off)
        s.why = off_why;
    s.state = state;
    if (!took_over)
        count.store(n + 1, std::memory_order_release);
    std::string from = record ? who + " (career.content record " + *record + ", no call)"
                       : took_over ? who + " (the call replaces the tile its career.content record " + own + " made)"
                                   : who;
    swofty::log("[career] content '%s' from %s registered (tile %zu after the stock ones, %s model, data group %s, suffix "
                "%s, mode %s): %s",
                desc.id.c_str(), from.c_str(), at + 1, desc.model == content::MODEL_2025 ? "2025" : "2026",
                desc.data_group.c_str(), desc.suffix.c_str(), desc.mode.c_str(),
                state == SWOFTY_CAREER_CONTENT_LIVE ? "live (hooks already installed)"
                : state == SWOFTY_CAREER_CONTENT_OFF ? ("off: " + off_why).c_str()
                                                     : "queued for the next mixin commit");
    return off ? SWOFTY_ASSET_UNAVAILABLE : SWOFTY_ASSET_OK;
}

void from_records()
{
    auto &st = runtime::older_store();
    for (auto &id : older::in_load_order(st, older::CONTENT)) {
        std::string addon = older::addon_of(id);
        if (addon.empty() || addon == "game" || addon == "swofty.core")
            continue;
        {
            std::lock_guard<std::mutex> g(lock);
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
            swofty::log("[career] career.content record %s from %s makes no tile: %s", id.c_str(), addon.c_str(),
                        why.c_str());
            continue;
        }
        content::Desc desc{d.id,   d.title, d.description, d.data_group, d.suffix, d.mode, d.entitlement,
                           d.model, d.car_group, d.badge, d.careers};
        add_slot(desc, addon, addon, &id);
    }
}

int api_add(const SwoftyCareerContentDesc *d)
{
    std::string who = caller(__builtin_return_address(0));
    constexpr size_t careers_end = offsetof(SwoftyCareerContentDesc, careers) + sizeof(uint32_t);
    if (!d || d->size < offsetof(SwoftyCareerContentDesc, careers)) {
        swofty::log("[career] content from %s refused: no descriptor, or its size %u is below %zu", who.c_str(),
                    d ? d->size : 0, offsetof(SwoftyCareerContentDesc, careers));
        return SWOFTY_ASSET_INVALID;
    }
    if (!runtime::enabled("career", nullptr)) {
        swofty::log("[career] content '%s' from %s refused: the career feature is off", text(d->id).c_str(), who.c_str());
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    content::Desc desc{text(d->id),   text(d->title), text(d->description), text(d->data_group),
                       text(d->suffix), text(d->mode), text(d->entitlement), d->model, d->car_group, d->badge,
                       d->size >= careers_end ? d->careers : content::CAREER_DRIVER};
    return add_slot(desc, who, runtime::older_addon(__builtin_return_address(0)), nullptr);
}

int api_info(const char *id, SwoftyCareerContentInfo *out)
{
    if (!id || !out)
        return SWOFTY_ASSET_INVALID;
    for (size_t i = 0; i < used(); i++) {
        Slot &s = slots[i];
        if (s.desc.id != id)
            continue;
        resolve(s, true);
        std::lock_guard<std::mutex> g(lock);
        *out = {};
        out->index = uint32_t(i);
        out->state = s.state;
        out->mode_stat_id = s.mode;
        out->data_group_index = s.group;
        out->tiles = s.tiles;
        out->presses = s.presses;
        copy_out(s.desc.id, out->id, sizeof out->id);
        copy_out(s.desc.data_group, out->data_group, sizeof out->data_group);
        copy_out(s.why, out->why, sizeof out->why);
        return SWOFTY_ASSET_OK;
    }
    return SWOFTY_ASSET_INVALID;
}

int api_data_group(const void *career, char *out, size_t cap)
{
    if (!out || !cap)
        return SWOFTY_ASSET_INVALID;
    out[0] = 0;
    if (!runtime::enabled("career", nullptr))
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!b.setup_desc)
        b.setup_desc = swofty::api->resolve("k_Career_CareerSetupComponentDesc");
    if (!b.setup_desc)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!career)
        career = runtime::career_table()->entity();
    if (!career)
        return SWOFTY_ASSET_LATE;
    content::GroupSeen seen = content::data_group(career, ACCESS);
    if (!seen.name)
        return SWOFTY_ASSET_INVALID;
    return copy_out(seen.name, out, cap);
}

int api_pressed(char *out, size_t cap)
{
    if (!out || !cap)
        return SWOFTY_ASSET_INVALID;
    out[0] = 0;
    if (!live)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> l(pressed_lock);
    if (!pressed_any)
        return SWOFTY_ASSET_LATE;
    return copy_out(pressed_group, out, cap);
}

const SwoftyCareerContent table = {api_available, api_add, api_info, api_data_group, api_pressed};

}

void turn_off(Slot &s, const std::string &why)
{
    {
        std::lock_guard<std::mutex> g(lock);
        s.why = why;
    }
    s.state = SWOFTY_CAREER_CONTENT_OFF;
    swofty::log("[career] content '%s' (%s) is off: %s", s.desc.id.c_str(), s.owner.c_str(), why.c_str());
}

size_t used()
{
    return count.load(std::memory_order_acquire);
}

void resolve_all(bool now, bool building)
{
    for (size_t i = 0; i < used(); i++)
        resolve(slots[i], now, building);
}

bool ready(const Slot &s)
{
    return s.state == SWOFTY_CAREER_CONTENT_LIVE && s.resolved;
}

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
                                    mode_id.empty() ? swofty::json::Value() : older::effective(st, older::MODE, mode_id),
                                    fresh, why))
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

using namespace career_content;

namespace runtime {

std::string career_content_owner(const std::string &data_group, std::string *save_name)
{
    std::lock_guard<std::mutex> g(lock);
    for (size_t i = 0; i < used(); i++)
        if (slots[i].desc.data_group == data_group) {
            if (save_name)
                *save_name = slots[i].desc.mode;
            return slots[i].owner;
        }
    return {};
}

}

extern "C" __declspec(dllexport) const SwoftyCareerContent *SwoftyCoreCareerContent(void)
{
    return &table;
}

namespace runtime {

const SwoftyCareerContent *career_content_table()
{
    return &table;
}

static std::vector<CoreMixins> career_content_mixins()
{
    std::vector<CoreMixins> out;
    from_records();
    if (mixin_gate.offered() || !used() || off || !code_ready())
        return out;
    mixin_gate.offer();
    std::string why;
    if (route_mixin_gate.settled() && !route_live)
        why = "the career tile route hook did not install (see the [core] lines for " + std::string(ROUTE_MIXIN) + ")";
    if (!why.empty() || !bind_all(why)) {
        off_why = why;
        off = true;
        for (size_t i = 0; i < used(); i++)
            turn_off(slots[i], why);
        return out;
    }
    out.push_back({MIXIN,
                   {{"site_FeTile_CareerContentEnd", SWOFTY_MIXIN_SITE, 100, (void *)&site_entry, nullptr},
                    {"Game_Career_IsDriverCareerMode", SWOFTY_MIXIN_WRAP, 100, (void *)&gate_entry, (void **)&gate_orig},
                    {"vt_Game_DriverCareerSlotPredicate", SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 2), 100,
                     (void *)&predicate_entry, (void **)&predicate_orig},
                    {"vt_Game_CareerDLCSelectViewModel", SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 10), 100,
                     (void *)&command_entry, (void **)&command_orig}}});
    tile_route_want();
    swofty::log("[career] contents: %zu registered; offering the career content hooks", used());
    return out;
}

void tile_route_want()
{
    route_wanted = true;
}

static std::vector<CoreMixins> tile_route_mixins()
{
    std::vector<CoreMixins> out;
    if (!route_wanted || route_mixin_gate.offered())
        return out;
    route_mixin_gate.offer();
    out.push_back({ROUTE_MIXIN,
                   {{"FeTile_Route_CareerItem", SWOFTY_MIXIN_WRAP, 100, (void *)&route_entry, (void **)&route_orig}}});
    return out;
}

static void tile_route_commit()
{
    if (!route_mixin_gate.offered() || route_mixin_gate.settled())
        return;
    route_live = core_mixins_active(ROUTE_MIXIN);
    route_mixin_gate.settle();
    swofty::log("[career] career tile route: %s", route_live ? "armed" : "off");
}

static loader::FunctionSet tile_route_core_set{loader::CoreSetId::tile_route, tile_route_mixins, tile_route_commit};

TileRoute tile_route_state()
{
    return route_live                                                             ? TileRoute::live
           : route_mixin_gate.settled() || (route_wanted && core_mixins_closed()) ? TileRoute::off
                                                                                  : TileRoute::pending;
}

static void career_content_commit()
{
    if (!route_mixin_gate.settled() || !mixin_gate.settle())
        return;
    if (core_mixins_active(MIXIN) && route_live) {
        live = true;
        for (size_t i = 0; i < used(); i++)
            if (slots[i].state == SWOFTY_CAREER_CONTENT_QUEUED)
                slots[i].state = SWOFTY_CAREER_CONTENT_LIVE;
        swofty::log("[career] contents: hooks installed; %zu content(s) live, their data is checked once the database "
                    "has loaded",
                    used());
        return;
    }
    off_why = "the career content hooks did not install (see the [core] lines for " +
              std::string(route_live ? MIXIN : ROUTE_MIXIN) + ")";
    off = true;
    for (size_t i = 0; i < used(); i++)
        if (slots[i].state != SWOFTY_CAREER_CONTENT_OFF)
            turn_off(slots[i], off_why);
}

static loader::FunctionSet career_content_core_set{loader::CoreSetId::career_content, career_content_mixins,
                                                   career_content_commit};

void career_content_tick()
{
    if (live && used())
        resolve_all(false);
}

}
