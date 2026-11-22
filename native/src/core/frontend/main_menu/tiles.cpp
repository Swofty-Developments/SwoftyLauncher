#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu.hpp"
#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/tile_list.hpp"
#include "core/frontend/tt_blocked.hpp"
#include "core/frontend/vm_string.hpp"

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <cstring>
#include <map>
#include <mutex>

namespace frontend::main_menu {

namespace {

using namespace swofty::game;

constexpr size_t OFF_F1WORLD = 11, OFF_CHALLENGE = 32, OFF_DIVIDER = 47, OFF_LOCAL_MP = 59, OFF_THEATRE = 98, OFF_END_JMP = 103;

const runtime::Sig S_BLOCK{"FeHome_F1WorldBlock",
                           "45 8D 7C 24 01 4C 89 F1 44 89 FA E8 ?? ?? ?? ?? F6 46 08 01 74 12 41 80 C4 02 4C 89 F1 44 "
                           "89 E2 E8 ?? ?? ?? ?? 45 89 E7 41 8D 57 01 4C 89 F1 E8 ?? ?? ?? ?? 41 8D 57 02 4C 89 F1 E8 "
                           "?? ?? ?? ?? 41 8D 57 03 4C 89 F1 E8 ?? ?? ?? ?? 41 8D 57 04 4C 89 F1 E8 ?? ?? ?? ?? 41 80 "
                           "C7 05 4C 89 F1 44 89 FA E8 ?? ?? ?? ?? E9",
                           0x1742fee,
                           runtime::Sig::addr,
                           0,
                           0,
                           0};

Add_t target(const uint8_t *at, size_t off)
{
    int32_t rel;
    std::memcpy(&rel, at + off + 1, 4);
    return reinterpret_cast<Add_t>(const_cast<uint8_t *>(at) + off + 5 + rel);
}

using frontend::tile_list::last_item;
using frontend::tile_list::route_action;
using frontend::vm::set_string;

void typed_populate(HomeItem *item, void *node, uint32_t a7) { menu.tiles.populate_typed(item, node, a7); }

void showroom_populate(HomeItem *item, void *node, uint32_t a7) { menu.tiles.populate_showroom(item, node, a7); }

Action *showroom_action(HomeItem *item, Action *out)
{
    menu.via = 2;
    swofty::log("[main-menu] showroom card pressed (item %p): route %s", item, SHOWROOM_ROUTE);
    return route_action(out, SHOWROOM_ROUTE);
}

void customisation_populate(HomeItem *item, void *node, uint32_t a7) { menu.tiles.populate_customisation(item, node, a7); }

Action *customisation_action(HomeItem *item, Action *out)
{
    menu.customisation_armed = true;
    swofty::log(
        "[main-menu] customisation card pressed (item %p): route %s; the Customisation screen's Back returns to the main menu",
        item, CUSTOMISATION_ROUTE);
    return route_action(out, CUSTOMISATION_ROUTE);
}

void split_screen_populate(HomeItem *item, void *node, uint32_t a7) { menu.tiles.populate_split_screen(item, node, a7); }

void card_populate(HomeItem *item, void *node, uint32_t a7) { menu.tiles.populate_card(item, node, a7); }

Action *card_action(HomeItem *item, Action *out) { return menu.navigation.card_pressed(item, out); }

}

const char *id_of(const HomeItem *item) { return item && item->id ? reinterpret_cast<const char *>(item->id) + NESTR_CHARS : ""; }

bool Tiles::resolve()
{
    std::call_once(once, [this] {
        if (!std::strcmp(env("SWOFTY_MAIN_MENU"), "0")) {
            swofty::log("[main-menu] off (SWOFTY_MAIN_MENU=0); the stock main menu stays");
            return;
        }
        const char *nav = env("SWOFTY_MAIN_MENU_NAV");
        swofty::log("[main-menu] navigation layout: %s%s", *nav ? nav : "single",
                    !std::strcmp(nav, "rows") ? " (rows is not built yet; single is used)" : "");
        auto at = reinterpret_cast<uint8_t *>(runtime::find(S_BLOCK, "home", "core"));
        if (!at) {
            swofty::log("[main-menu] the home tile block did not resolve on this build; the stock main menu stays");
            return;
        }
        if (at[OFF_END_JMP] != 0xE9) {
            swofty::log("[main-menu] the home tile block has no closing jmp; the stock main menu stays");
            return;
        }
        block.at = at;
        block.f1world = target(at, OFF_F1WORLD);
        block.challenge = target(at, OFF_CHALLENGE);
        block.divider = target(at, OFF_DIVIDER);
        block.local_mp = target(at, OFF_LOCAL_MP);
        block.theatre = target(at, OFF_THEATRE);
        swofty::log("[main-menu] home tile block at %p; adders F1 World %p, challenge %p, Local Multiplayer %p, Theatre %p", at,
                    reinterpret_cast<void *>(block.f1world), reinterpret_cast<void *>(block.challenge),
                    reinterpret_cast<void *>(block.local_mp), reinterpret_cast<void *>(block.theatre));
        resolved = true;
    });
    return resolved.load();
}

bool Tiles::rows_on()
{
    int f = rows_flag.load();
    if (f < 0) {
        f = runtime::read_file(runtime::under_swofty("ui\\home_rows.flag"), 16).empty() ? 0 : 1;
        rows_flag = f;
        swofty::log("[main-menu] the launcher's rows of cards %s",
                    f ? "are in this build" : "are not in this build; the tiles stay in the stock list");
    }
    return f == 1;
}

bool Tiles::menu_on()
{
    int f = menu_flag.load();
    if (f < 0) {
        f = runtime::read_file(runtime::under_swofty("ui\\home_menu.flag"), 16).empty() ? 0 : 1;
        menu_flag = f;
        swofty::log("[main-menu] the launcher's Showroom route %s",
                    f ? "is in this build" : "is not in this build; no Showroom card");
    }
    return f == 1;
}

bool Tiles::customisation_on()
{
    int f = customisation_flag.load();
    if (f < 0) {
        f = runtime::read_file(runtime::under_swofty("ui\\home_customisation.flag"), 16).empty() ? 0 : 1;
        customisation_flag = f;
        swofty::log("[main-menu] the launcher's Customisation routes %s",
                    f ? "are in this build" : "are not in this build; no Customisation card");
    }
    return f == 1;
}

void Tiles::style_item(HomeItem *item)
{
    const char *tpl = template_for(id_of(item), rows_on());
    auto node = item ? static_cast<uint8_t *>(item->button_node.node) : nullptr;
    if (!*tpl)
        return;
    if (!node) {
        swofty::log("[main-menu] %s: no VM node yet, template %s not set", id_of(item), tpl);
        return;
    }
    set_string(*g_Vm_Registry, node + 8, "type", tpl);
    swofty::log("[main-menu] %s -> template %s", id_of(item), tpl);
}

void Tiles::populate_typed(HomeItem *item, void *node, uint32_t a7)
{
    void *orig = nullptr;
    {
        std::lock_guard<std::mutex> g(typed_lock);
        auto it = typed_orig.find(reinterpret_cast<void **>(item->vtable));
        orig = it == typed_orig.end() ? nullptr : it->second;
    }
    if (orig)
        reinterpret_cast<Populate_t>(orig)(item, node, a7);
    style_item(item);
}

void Tiles::wrap_item(HomeItem *item)
{
    const char *id = id_of(item);
    if (!*template_for(id, rows_on()))
        return;
    if (!std::strcmp(id, ID_GRAND_PRIX) || !std::strcmp(id, ID_TIME_TRIAL) || !std::strcmp(id, ID_LOCAL) ||
        !std::strcmp(id, ID_SHOWROOM) || !std::strcmp(id, ID_CUSTOMISATION) || !std::strcmp(id, ID_SETTINGS))
        return;
    auto vt = reinterpret_cast<void **>(item->vtable);
    {
        std::lock_guard<std::mutex> g(typed_lock);
        if (typed_orig.count(vt))
            return;
    }
    void *orig = vt[4];
    void **nv = swofty::subclass(item, {{4, reinterpret_cast<void *>(typed_populate)}});
    std::lock_guard<std::mutex> g(typed_lock);
    typed_orig[nv] = orig;
    swofty::log("[main-menu] %s: populate wrapped to set its template", id);
}

void Tiles::style_list(TileList *list)
{
    for (auto e = list->items_begin; e && e < list->items_end; e++)
        if (e->tile && e->tile->item)
            wrap_item(e->tile->item);
}

void Tiles::retitle(HomeItem *item, const char *title, const char *description)
{
    auto vm_node = static_cast<uint8_t *>(item->button_node.node);
    if (!vm_node)
        return;
    void *reg = *g_Vm_Registry, *parent = vm_node + 8;
    uint8_t no = 0;
    set_string(reg, parent, "title", title);
    Vm_Set_Bool(reg, parent, "title_localised", &no, 1);
    set_string(reg, parent, "description", description);
    Vm_Set_Bool(reg, parent, "description_localised", &no, 1);
    style_item(item);
}

void Tiles::populate_showroom(HomeItem *item, void *node, uint32_t a7)
{
    reinterpret_cast<Populate_t>(theatre_populate)(item, node, a7);
    retitle(item, "SHOWROOM", "Look over every car in the game");
}

void Tiles::populate_customisation(HomeItem *item, void *node, uint32_t a7)
{
    reinterpret_cast<Populate_t>(theatre_populate)(item, node, a7);
    retitle(item, CUSTOMISATION_TITLE, CUSTOMISATION_DESCRIPTION);
}

void Tiles::add_theatre_copy(TileList *list, uint8_t index, const char *id, Populate_t populate, GetAction_t action)
{
    block.theatre(list, index);
    HomeItem *item = last_item(list);
    if (!item)
        return;
    auto vt = reinterpret_cast<void **>(item->vtable);
    if (!theatre_populate) {
        theatre_populate = vt[4];
        theatre_action = vt[5];
    }
    if (vt[4] != theatre_populate) {
        swofty::log("[main-menu] %s: the Theatre item's vtable changed; left as a Theatre tile", id);
        return;
    }
    swofty::subclass(item, {{4, reinterpret_cast<void *>(populate)}, {5, reinterpret_cast<void *>(action)}});
    Ne_Make_StrFromCStr(&item->id, id, 0, 0);
}

void Tiles::populate_split_screen(HomeItem *item, void *node, uint32_t a7)
{
    reinterpret_cast<Populate_t>(local_mp_populate)(item, node, a7);
    retitle(item, SPLIT_SCREEN_TITLE, SPLIT_SCREEN_DESCRIPTION);
}

// The stock Local Multiplayer item, titled Split Screen: its id and its action (the local_multiplayer route, which
// opens the game's Local Multiplayer list, the lone Splitscreen tile) stay the game's.
void Tiles::add_split_screen(TileList *list, uint8_t index)
{
    block.local_mp(list, index);
    HomeItem *item = last_item(list);
    if (!item) {
        swofty::log("[main-menu] split screen: the Local Multiplayer adder left no item");
        return;
    }
    auto vt = reinterpret_cast<void **>(item->vtable);
    if (!local_mp_populate)
        local_mp_populate = vt[4];
    if (vt[4] != local_mp_populate) {
        swofty::log("[main-menu] split screen: the Local Multiplayer item's vtable changed; left as a Local Multiplayer tile");
        return;
    }
    swofty::subclass(item, {{4, reinterpret_cast<void *>(split_screen_populate)}});
}

void Tiles::populate_card(HomeItem *item, void *node, uint32_t a7)
{
    reinterpret_cast<Populate_t>(f1w_populate)(item, node, a7);
    auto vm_node = static_cast<uint8_t *>(item->button_node.node);
    if (!vm_node)
        return;
    bool gp = !std::strcmp(id_of(item), ID_GRAND_PRIX);
    void *reg = *g_Vm_Registry, *parent = vm_node + 8;
    uint8_t no = 0;
    set_string(reg, parent, "title", gp ? "GRAND PRIX" : "TIME TRIAL");
    Vm_Set_Bool(reg, parent, "title_localised", &no, 1);
    const char *blocked = gp ? nullptr : tt_blocked::card_description();
    set_string(reg, parent, "description",
               gp        ? "Race a Grand Prix weekend"
               : blocked ? blocked
                         : "Set the fastest lap on any track");
    Vm_Set_Bool(reg, parent, "description_localised", &no, 1);
    style_item(item);
}

void Tiles::add_card(TileList *list, uint8_t index, const char *id)
{
    block.f1world(list, index);
    HomeItem *item = last_item(list);
    if (!item) {
        swofty::log("[main-menu] %s: the adder left no item", id);
        return;
    }
    auto vt = reinterpret_cast<void **>(item->vtable);
    if (!f1w_populate) {
        f1w_populate = vt[4];
        f1w_action = vt[5];
    }
    if (vt[4] != f1w_populate || vt[5] != f1w_action) {
        swofty::log("[main-menu] %s: the item's vtable is not the F1 World one; left as a plain F1 World tile", id);
        return;
    }
    swofty::subclass(item, {{4, reinterpret_cast<void *>(card_populate)}, {5, reinterpret_cast<void *>(card_action)}});
    Ne_Make_StrFromCStr(&item->id, id, 0, 0);
}

bool Tiles::skip_stock_block()
{
    uint8_t jmp[5] = {0xE9, uint8_t(OFF_END_JMP - 5), 0, 0, 0};
    DWORD old;
    if (!VirtualProtect(block.at, sizeof jmp, PAGE_EXECUTE_READWRITE, &old)) {
        swofty::log("[main-menu] could not make the home tile block writable; the stock main menu stays");
        resolved = false;
        return false;
    }
    std::memcpy(block.at, jmp, sizeof jmp);
    VirtualProtect(block.at, sizeof jmp, old, &old);
    FlushInstructionCache(GetCurrentProcess(), block.at, sizeof jmp);
    return true;
}

uint8_t Tiles::add(TileList *list, uint8_t career_index, const uint8_t *flags)
{
    if (!arm_patch())
        return career_index;
    uint8_t index = career_index;
    add_card(list, ++index, ID_GRAND_PRIX);
    add_card(list, ++index, ID_TIME_TRIAL);
    add_split_screen(list, ++index);
    if (flags && (flags[8] & 1))
        block.challenge(list, ++index);
    block.theatre(list, ++index);
    bool showroom = menu_on();
    if (showroom)
        add_theatre_copy(list, ++index, ID_SHOWROOM, showroom_populate, showroom_action);
    bool customisation = customisation_on();
    if (customisation)
        add_theatre_copy(list, ++index, ID_CUSTOMISATION, customisation_populate, customisation_action);
    block.divider(list, ++index);
    if (HomeItem *divider = last_item(list)) {
        Ne_Make_StrFromCStr(&divider->id, ID_DIVIDER, 0, 0);
        swofty::log("[main-menu] divider tile added before Addon Settings (index %u)", unsigned(index));
    }
    style_list(list);
    swofty::log("[main-menu] tiles added after Career: Grand Prix, Time Trial, Split Screen%s, Theatre%s%s (last index %u)",
                flags && (flags[8] & 1) ? ", challenge" : "", showroom ? ", Showroom" : "",
                customisation ? ", Customisation" : "", unsigned(index));
    return index;
}

}
