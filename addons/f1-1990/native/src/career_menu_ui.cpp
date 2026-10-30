// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - a third "1990 Career" tile on the career content select menu, next to the stock 2025
// and 2026 tiles. Its own DLL, so a binding it can't resolve drops only this hook.
//
// FeTile_Build_List case 6 builds the 2025 and 2026 content tiles inline; at its end
// (site_FeTile_CareerContentEnd, r14 = the TileList) FeHome_Add_CareerTile appends a third tile
// (button_3, template fe_tile_button_default, its own VM node and list entry). Its 0x148-byte home
// item is replaced, the way case 6 replaces items, by a career content item built like the 2026 one
// (FeTile_Construct_CareerItem, 0x1c8 bytes) with the 2026 tile's route (+0x178, driver_career), data group
// f1_career_1990, suffix 90 and a copy of the 2026 vtable whose populate shows the 1990 labels with
// no badge and whose press selects the 1990 car group (or the 2026 one) before the router. The
// populate loop after the site then initialises and populates all three tiles. Only when the 1990
// career content is in the database; option career_menu_ui turns it off.
//
// The wrap of FeTile_Route_CareerItem, which every career tile press ends in, records whether the
// last press was the 1990 tile; F1990_CareerWizard1990 exports that for the other 1990 DLLs.
//
// Built against an SDK with swofty/career_content.hpp, all of the above is the core's: this DLL
// registers the 1990 content (data group f1_career_1990, suffix 90, mode driver_career_1990, car
// group 2, 2026 model) and the tile, press, route, gate pass and slot predicate below compile out.
// F1990_CareerWizard1990 then answers from the core's last pressed content tile. Against an older
// SDK the private hooks below are built as before.
//
// The router finds the career's save slot with a predicate functor (vt_Game_DriverCareerSlotPredicate
// slot 2) over the registered mode championships. Its first gate (0x14077cd40, VM-obfuscated) is 1
// only for the 2025 and 2026 driver-career modes, so the 1990 mode never matched and the new-career
// path read through a null slot. When the predicate, asked for f1_career_1990, says no for the mode
// championship whose statId (+0x2a8) is the driver_career_1990 mode's (read from game_modes), the
// wrap runs the predicate's own data group check (career_group.hpp) on it instead. Questions for
// any other data group keep the game's answer.

#include <atomic>
#include <cstring>

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#if __has_include(<swofty/career_content.hpp>)
#include <swofty/career_content.hpp>
#define F1990_CONTENT_API 1
#else
#define F1990_CONTENT_API 0
#endif

#include "career_1990.hpp"
#include "content_entry.hpp"
#include "profile.hpp"
#include "weekend.hpp"

SWOFTY_SETTING(bool, career_menu_ui, true);

#if F1990_CONTENT_API

namespace {

namespace content = swofty::career::content;

bool register_content()
{
    if (!career_menu_ui) {
        swofty::log("[f1-1990] career menu UI: 1990 Career tile off (career_menu_ui)");
        return true;
    }
    content::Result r = content::add({.id = "f1-1990",
                                      .title = "lng_f1990_career_content",
                                      .description = "lng_f1990_career_content_description",
                                      .data_group = f1990::kCareerGroup1990,
                                      .suffix = "90",
                                      .mode = "driver_career_1990",
                                      .car_group = f1990::kCarGroup1990});
    swofty::log("[f1-1990] career menu UI: 1990 Career content registered with the core: %s", swofty::career::name(r));
    return true;
}

}

extern "C" __declspec(dllexport) int F1990_CareerWizard1990()
{
    return swofty::career::content::pressed_is(f1990::kCareerGroup1990) ? 1 : 0;
}

SWOFTY_ADDON(register_content)

#else

namespace {

using f1990::at;
using swofty::game::Action;
using swofty::game::HomeItem;
using swofty::game::NeAllocator;
using swofty::game::NeStr;
using swofty::game::Tile;
using swofty::game::TileList;

constexpr uint8_t kIndex = 3;
constexpr size_t kItemSize = 0x1c8;
constexpr size_t kSlots = 22;
constexpr size_t kPopulateSlot = 4;
constexpr size_t kPressSlot = 5;
constexpr size_t kNotificationSlot = 0x78 / 8;
constexpr char kTitle[] = "lng_f1990_career_content";
constexpr char kDescription[] = "lng_f1990_career_content_description";

void **g_vtable;
TileList *g_list;
std::atomic<uint32_t> g_builds, g_presses;
std::atomic<bool> g_wizard_1990{false};
std::atomic<int64_t> g_mode_1990{-2};
std::atomic<uint32_t> g_slot_logged, g_gate_logged;

constexpr char kMode1990Save[] = "driver_career_1990";

void set_text(void *node, const char *name, const char *text)
{
    NeStr *s = nullptr;
    swofty::game::Ne_Make_StrFromCStr(&s, text, 0, 0);
    swofty::game::Vm_Set_String(*swofty::game::g_Vm_Registry, node, name, &s, 1);
    swofty::game::Ne_Make_StrFromCStr(&s, nullptr, 0, 0);
}

void populate(HomeItem *item, void *node, uint32_t a7)
{
    swofty::game::FeTile_CareerDlcItem_Populate.get()(item, node, a7);
    auto *vm = static_cast<uint8_t *>(item->button_node.node);
    if (!vm)
        return;
    set_text(vm + 8, "title", kTitle);
    set_text(vm + 8, "description", kDescription);
    auto **vt = *reinterpret_cast<void ***>(item);
    reinterpret_cast<void (*)(HomeItem *, int)>(vt[kNotificationSlot])(item, 0);
}

bool is_dlc_item(const HomeItem *item);

bool adopt_systems(HomeItem *item)
{
    auto *raw = reinterpret_cast<uint8_t *>(item);
    if (at<void *>(raw, 0x158))
        return true;
    TileList *list = g_list;
    if (!list || !list->items_begin || list->items_end - list->items_begin < 2 || !list->items_end[-1].tile ||
        list->items_end[-1].tile->item != item || !list->items_end[-2].tile)
        return false;
    HomeItem *dlc = list->items_end[-2].tile->item;
    auto *src = reinterpret_cast<const uint8_t *>(dlc);
    if (!is_dlc_item(dlc) || !at<void *>(src, 0x158))
        return false;
    std::memcpy(raw + 0x150, src + 0x150, 0x28);
    swofty::log("[f1-1990] career menu: 1990 item took the 2026 tile's menu systems");
    return true;
}

Action *press(HomeItem *item, Action *out)
{
    auto *system = static_cast<const uint8_t *>(*swofty::game::g_FeTile_CustomisationSystem);
    auto *b = system ? at<const uint8_t *>(system, 0x230) : nullptr;
    auto *e = system ? at<const uint8_t *>(system, 0x238) : nullptr;
    f1990::ContentPick pick = f1990::pick_content(b, e, f1990::kCarGroup1990);
    *swofty::game::g_FeTile_SelectedCareerContent = pick.id;
    if (g_presses++ < 4)
        swofty::log("[f1-1990] career menu: 1990 Career pressed; content 0x%08x (%s)", pick.id,
                    pick.kind == f1990::ContentPick::exact ? "1990 car group"
                    : pick.kind == f1990::ContentPick::dlc ? "no 1990 car group, the 2026 one"
                                                           : "no content entry");
    if (!adopt_systems(item)) {
        out->type = 1;
        *reinterpret_cast<void **>(reinterpret_cast<uint8_t *>(out) + 8) = nullptr;
        swofty::log("[f1-1990] career menu: the 1990 item has no menu systems and the 2026 tile has none to copy; press ignored");
        return out;
    }
    return swofty::game::FeTile_Route_CareerItem(item, out);
}

void set_string(void *field, const char *text)
{
    swofty::game::Ne_Make_StrFromCStr(static_cast<NeStr **>(field), text, 0, 0);
}

HomeItem *build_item(NeAllocator *alloc, HomeItem *dlc)
{
    auto alloc_fn = reinterpret_cast<void *(*)(NeAllocator *, size_t, size_t)>(alloc->vtable->alloc);
    auto *item = static_cast<HomeItem *>(alloc_fn(alloc, kItemSize, 0x10));
    if (!item)
        return nullptr;
    const char *id = dlc->id ? dlc->id->chars : "career_dlc_content";
    swofty::game::FeTile_Construct_CareerItem(item, alloc, id);
    auto *raw = reinterpret_cast<uint8_t *>(item);
    auto *route = at<const uint8_t *>(dlc, 0x178);
    set_string(raw + 0x178, route && route[9] ? reinterpret_cast<const char *>(route + 9) : "driver_career");
    std::memset(raw + 0x1b8, 0, 0x10);
    set_string(raw + 0x1c0, "f1_2026");
    set_string(raw + 0x180, "f1_career_1990");
    set_string(raw + 0x188, "90");
    raw[0x190] = 0;
    if (!g_vtable) {
        auto **src = *reinterpret_cast<void ***>(dlc);
        auto **copy = new void *[kSlots + 1];
        std::memcpy(copy, src - 1, (kSlots + 1) * sizeof(void *));
        copy[1 + kPopulateSlot] = reinterpret_cast<void *>(&populate);
        copy[1 + kPressSlot] = reinterpret_cast<void *>(&press);
        g_vtable = copy + 1;
    }
    *reinterpret_cast<void ***>(item) = g_vtable;
    return item;
}

bool is_dlc_item(const HomeItem *item)
{
    auto *const *vt = item ? *reinterpret_cast<void *const *const *>(item) : nullptr;
    return vt && vt[kPopulateSlot] == reinterpret_cast<void *>(swofty::game::FeTile_CareerDlcItem_Populate.address());
}

void add_tile(TileList *list)
{
    if (!list || !list->items_begin || list->items_end <= list->items_begin)
        return;
    Tile *last = list->items_end[-1].tile;
    HomeItem *dlc = last ? last->item : nullptr;
    if (!is_dlc_item(dlc)) {
        swofty::log("[f1-1990] career menu: the content list does not end with the 2026 tile; no 1990 tile");
        return;
    }
    size_t before = size_t(list->items_end - list->items_begin);
    swofty::game::FeHome_Add_CareerTile(list, kIndex);
    size_t after = list->items_begin ? size_t(list->items_end - list->items_begin) : 0;
    Tile *tile = after == before + 1 ? list->items_end[-1].tile : nullptr;
    if (!tile || !tile->item || !tile->allocator) {
        swofty::log("[f1-1990] career menu: the tile adder gave %zu -> %zu items; no 1990 tile", before, after);
        return;
    }
    NeAllocator *alloc = at<NeAllocator *>(list, 0x08);
    HomeItem *item = alloc ? build_item(alloc, dlc) : nullptr;
    if (!item) {
        swofty::log("[f1-1990] career menu: could not build the 1990 content item; the third tile stays a Career tile");
        return;
    }
    HomeItem *old = tile->item;
    NeAllocator *old_alloc = tile->allocator;
    tile->item = item;
    tile->allocator = alloc;
    reinterpret_cast<void (*)(HomeItem *, int)>(old->vtable->destroy)(old, 0);
    reinterpret_cast<void (*)(NeAllocator *, void *)>(old_alloc->vtable->free)(old_alloc, old);
    g_list = list;
    if (g_builds++ < 4)
        swofty::log("[f1-1990] career menu: 1990 Career tile added as button_%u (items %zu -> %zu)", kIndex, before,
                    after);
}

int64_t mode_1990()
{
    int64_t mode = g_mode_1990.load();
    if (mode != -2 || !swofty::db::available())
        return mode == -2 ? -1 : mode;
    mode = f1990::mode_stat_id(kMode1990Save);
    g_mode_1990 = mode;
    swofty::log("[f1-1990] career slot: driver_career_1990 mode statId %lld", (long long)mode);
    return mode;
}

bool is_1990_mode(const void *championship)
{
    int64_t mode = championship ? mode_1990() : -1;
    return mode >= 0 && at<uint32_t>(championship, 0x2a8) == uint32_t(mode);
}

}

SWOFTY_WRAP(FeTile_Route_CareerItem, Action *, HomeItem *item, Action *out)
{
    g_wizard_1990 = g_vtable && item && *reinterpret_cast<void ***>(item) == g_vtable;
    return orig(item, out);
}

SWOFTY_WRAP(Game_Career_IsDriverCareerMode, bool, const void *championship)
{
    if (!is_1990_mode(championship))
        return orig(championship);
    if (g_gate_logged++ < 8) {
        f1990::CareerGroupSeen seen =
            f1990::career_data_group_seen(championship, {f1990::setup_component, f1990::group_index});
        swofty::log("[f1-1990] career slot: the 1990 mode passes the driver-career gate; data group %d of %zu is \"%s\"",
                    seen.index, seen.count, seen.name ? seen.name : "(none)");
    }
    return true;
}

SWOFTY_VTABLE(vt_Game_DriverCareerSlotPredicate, 2, bool, void *self, void *championship)
{
    bool stock = orig(self, championship);
    if (stock || !championship)
        return stock;
    int64_t mode = mode_1990();
    if (!is_1990_mode(championship))
        return false;
    auto *holder = at<const uint8_t *>(self, 0x08);
    const char *want = holder ? at<const char *>(holder, 0x00) : nullptr;
    if (!want || std::strcmp(want, f1990::kCareerGroup1990))
        return false;
    f1990::CareerGroupAccess access{f1990::setup_component, f1990::group_index};
    bool ok = want && f1990::career_data_group_is(championship, want, access);
    if (g_slot_logged++ < 8) {
        f1990::CareerGroupSeen seen = f1990::career_data_group_seen(championship, access);
        swofty::log("[f1-1990] career slot: the 1990 mode (statId %lld) failed the driver-career gate; data group %d of "
                    "%zu is \"%s\", wanted \"%s\": %s",
                    (long long)mode, seen.index, seen.count, seen.name ? seen.name : "(none)", want ? want : "(none)",
                    ok ? "match" : "no match");
    }
    return ok;
}

extern "C" __declspec(dllexport) int F1990_CareerWizard1990()
{
    return g_wizard_1990.load() ? 1 : 0;
}

SWOFTY_SITE(site_FeTile_CareerContentEnd, swofty::Context &c)
{
    if (!career_menu_ui || f1990::has_1990_content("career menu") != 1)
        return;
    add_tile(reinterpret_cast<TileList *>(c.r14));
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] career menu UI loaded: 1990 Career tile %s", career_menu_ui ? "on" : "off (career_menu_ui)");
}

SWOFTY_ADDON()

#endif
