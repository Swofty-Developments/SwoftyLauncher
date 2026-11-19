#include "core/career/content_state.hpp"
#include "core/career/resume_list.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <cstddef>
#include <mutex>
#include <string>

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

namespace career::content {

namespace {

using RegistryGet = void *(void *, const void *, int);
using GroupIndex = uint8_t(void *);
using MakeStr = uint32_t(NeStr **, const char *, uint32_t, uintptr_t);

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

bool readable(const void *p, size_t n, void *) { return mem::readable(p, n); }

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

namespace {

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

}

std::string suffix_of_group(const std::string &group)
{
    for (size_t i = 0; i < used(); i++)
        if (slots[i].desc.data_group == group)
            return slots[i].desc.suffix;
    return {};
}

namespace {

void redirect_resume(Action *out)
{
    namespace rl = career::resume_list;
    const char *route = out && out->type == 1 ? nestr(out->str) : nullptr;
    bool my_team = false;
    if (!route || !rl::armed() || !rl::continue_route_of(route, my_team))
        return;
    size_t n = rl::rows();
    if (rl::new_career_requested()) {
        swofty::log("[career] resume screen: route %s opens the stock continue screen: New Career was chosen on the list screen",
                    route);
        return;
    }
    if (!rl::redirects(true, n, route, my_team)) {
        swofty::log("[career] resume screen: settings.json lists no row for the list screen; the stock Resume Career screen "
                    "opens for route %s",
                    route);
        return;
    }
    reinterpret_cast<MakeStr *>(b.make_str)(&out->str, rl::LIST_ROUTE, 0, 0);
    swofty::log("[career] resume screen: Resume Career (route %s) is rewritten to route %s, the list screen with %zu row(s)",
                route, rl::LIST_ROUTE, n);
}

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
    Action *r = route.route_item(item, out);
    runtime::wizard_tile_routed(g, out);
    redirect_resume(out);
    return r;
}

bool gate_entry(const void *championship)
{
    Slot *s = used() ? by_mode(championship) : nullptr;
    if (!s)
        return hooks.driver_gate(championship);
    if (s->gates++ < LOG_FIRST)
        swofty::log("[career] content '%s': its mode (statId %lld) passes the driver-career gate", s->desc.id.c_str(),
                    (long long)s->mode.load());
    return true;
}

bool predicate_entry(void *self, void *championship)
{
    bool stock = hooks.slot_predicate(self, championship);
    if (stock || !championship || !used())
        return stock;
    Slot *s = by_mode(championship);
    auto *holder = s && mem::readable(static_cast<uint8_t *>(self) + 8, 8) ? content::at<const uint8_t *>(self, 0x08) : nullptr;
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
    bool y25 = false, y26 = false;
    for (size_t i = 0; i < used(); i++)
        (slots[i].desc.model == content::MODEL_2025 ? y25 : y26) = true;
    if (!used())
        y26 = true;
    return ContentHooks::bind({{"site_FeTile_CareerContentEnd", &b.site},
                               {"FeTile_Route_CareerItem", &b.route},
                               {"Game_Career_IsDriverCareerMode", &b.gate},
                               {"vt_Game_DriverCareerSlotPredicate", &b.predicate_vt},
                               {"vt_Game_CareerDLCSelectViewModel", &b.select_vt},
                               {"FeHome_Add_CareerTile", &b.add_tile},
                               {"FeTile_Construct_CareerItem", &b.construct},
                               {y26 ? "FeTile_CareerDlcItem_Populate" : nullptr, &b.populate_2026},
                               {y25 ? "FeTile_CareerDefaultItem_Populate" : nullptr, &b.populate_2025},
                               {"g_FeTile_CustomisationSystem", &b.customisation},
                               {"g_FeTile_SelectedCareerContent", &b.selected},
                               {"k_Career_CareerSetupComponentDesc", &b.setup_desc},
                               {"Ne_Make_StrFromCStr", &b.make_str},
                               {"Vm_Set_String", &b.set_string},
                               {"g_Vm_Registry", &b.vm_registry},
                               {"Vm_GetOrCreate_ListChild", &b.list_child, false},
                               {"Vm_Set_Bool", &b.set_bool, false}},
                              why);
}

}
