#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <mutex>
#include <string>
#include <unordered_map>

#include "core/frontend/hidden_tiles.hpp"
#include "core/frontend/main_menu.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/tile_list.hpp"
#include "core/frontend/tile_records_read.hpp"
#include "core/frontend/vm_string.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"
#include "core/runtime/loader/core_rank.hpp"

using namespace swofty::game;

#if defined(SWOFTY_NO_MOD_SETTINGS)
#define SWOFTY_MOD_SETTINGS_MODE "compiled out (diagnostic build: no site hook, no tile)"
#elif defined(SWOFTY_MOD_SETTINGS_NOOP)
#define SWOFTY_MOD_SETTINGS_MODE "site hook installed, handler is a no-op (diagnostic build: no tile)"
#else
#define SWOFTY_MOD_SETTINGS_MODE "active"
#define SWOFTY_MOD_SETTINGS_ACTIVE 1
#endif

SWOFTY_CORE_RANK(home_tiles)

namespace frontend {

namespace {

static_assert(sizeof(TileEntry) == 0x10);

unsigned long long rva(uintptr_t a) { return a - swofty::api->module_base(); }

#ifdef SWOFTY_MOD_SETTINGS_ACTIVE

constexpr const char *ROUTE = "mod_menu";
constexpr const char *TITLE = "ADDON SETTINGS";
constexpr const char *DESCRIPTION = "Configure the addons installed by Swofty Launcher";

struct SettingsTile {
    unsigned hook_calls = 0, populate_calls = 0, action_calls = 0;
    void **mod_vt = nullptr;
};

constinit SettingsTile tile;

struct RecordTile {
    std::string title, route;
    bool localised = true;
};

std::mutex record_lock;
std::unordered_map<const HomeItem *, RecordTile> record_tiles;
constinit swofty_core::guards::LogFirst<4> record_lines, image_lines, world_lines;

bool record_of(const HomeItem *item, RecordTile &out)
{
    std::lock_guard<std::mutex> g(record_lock);
    auto it = record_tiles.find(item);
    if (it == record_tiles.end())
        return false;
    out = it->second;
    return true;
}

void mod_populate(HomeItem *item, void *node, uint32_t a7)
{
    unsigned call = ++tile.populate_calls;
    FeTile_ItemBase_Populate(item, node, a7);
    auto vm_node = static_cast<uint8_t *>(item->button_node.node);
    if (!vm_node) {
        swofty::log("[core] populate #%u item=%p: no VM node after base populate, label left as is", call, item);
        return;
    }
    void *reg = *g_Vm_Registry, *parent = vm_node + 8;
    uint8_t no = 0;
    RecordTile rec;
    if (record_of(item, rec)) {
        uint8_t localised = rec.localised ? 1 : 0;
        frontend::vm::set_string(reg, parent, "title", rec.title.c_str());
        Vm_Set_Bool(reg, parent, "title_localised", &localised, 1);
        frontend::vm::set_string(reg, parent, "description", "");
        Vm_Set_Bool(reg, parent, "description_localised", &no, 1);
        swofty::log("[core] populate #%u item=%p: ui.tile title '%s' (%s)", call, item, rec.title.c_str(),
                    rec.localised ? "lang key" : "literal");
        return;
    }
    frontend::vm::set_string(reg, parent, "title", TITLE);
    Vm_Set_Bool(reg, parent, "title_localised", &no, 1);
    frontend::vm::set_string(reg, parent, "description", DESCRIPTION);
    Vm_Set_Bool(reg, parent, "description_localised", &no, 1);
    swofty::log("[core] populate #%u item=%p node=%p a7=%u vm=%p: title='%s' description set, localised=false", call, item, node,
                a7, vm_node, TITLE);
    frontend::main_menu::style_settings(item);
}

Action *mod_get_action(HomeItem *item, Action *out)
{
    RecordTile rec;
    const char *route = record_of(item, rec) ? rec.route.c_str() : ROUTE;
    frontend::tile_list::route_action(out, route);
    swofty::log("[core] getAction #%u item=%p -> {type 1, route '%s'}", ++tile.action_calls, item, route);
    return out;
}

#endif

bool check()
{
    runtime::loader::rank_core_mixins();
    uintptr_t site = site_FeHome_AfterCareerCall.address(), builder = FeTile_Build_List.address();
    swofty::log("[core] swofty_core init, game base 0x%llx", (unsigned long long)swofty::api->module_base());
    swofty::log("[core] ADDON SETTINGS mixin: %s", SWOFTY_MOD_SETTINGS_MODE);
    if (site < builder || site - builder > 0x8000) {
        swofty::log("[core] hook site 0x%llx is not inside the builder at 0x%llx; installing nothing", (unsigned long long)site,
                    (unsigned long long)builder);
        return false;
    }
    swofty::log("[core] builder base+0x%llx, hook site base+0x%llx", rva(builder), rva(site));
    swofty::log("[core] career_adder base+0x%llx, base_populate base+0x%llx, nestr_from base+0x%llx",
                rva(FeHome_Add_CareerTile.address()), rva(FeTile_ItemBase_Populate.address()),
                rva(Ne_Make_StrFromCStr.address()));
    swofty::log("[core] registry global base+0x%llx (= %p now), vm_set_string base+0x%llx, vm_set_bool base+0x%llx",
                rva(g_Vm_Registry.address()), *g_Vm_Registry, rva(Vm_Set_String.address()), rva(Vm_Set_Bool.address()));
    return true;
}

}

#if defined(SWOFTY_MOD_SETTINGS_NOOP)

SWOFTY_SITE(site_FeHome_AfterCareerCall, swofty::Context &c)
{
    static unsigned calls;
    swofty::log("[core] hook #%u (no-op): list(r14)=0x%llx r12=0x%llx r15=0x%llx; nothing added", ++calls,
                (unsigned long long)c.r14, (unsigned long long)c.r12, (unsigned long long)c.r15);
}

#elif defined(SWOFTY_MOD_SETTINGS_ACTIVE)

void add_record_tiles(swofty::Context &c, TileList *list, void **career_vt, unsigned call)
{
    {
        std::lock_guard<std::mutex> g(record_lock);
        record_tiles.clear();
    }
    frontend::tile_records::Options opt;
    opt.need_route = true;
    size_t used = size_t(c.r12 & 0xff) + 1;
    opt.cap = used >= frontend::tile_records::TILE_CAP ? 0 : frontend::tile_records::TILE_CAP - used;
    auto found = frontend::tile_records::read(frontend::tile_records::Place::home, opt);
    for (auto &line : found.lines)
        if (auto n = record_lines.note())
            swofty::log("[ui.tile] %s%s", line.c_str(), n.tail ? n.tail : "");
    size_t images = 0, added = 0;
    for (auto &t : found.tiles) {
        size_t before = 0, after = 0;
        frontend::tile_list::last_item(list, &before);
        uint8_t index = uint8_t(c.r12 + 1);
        FeHome_Add_CareerTile(list, index);
        HomeItem *item = frontend::tile_list::last_item(list, &after);
        if (!item || after != before + 1 || reinterpret_cast<void **>(item->vtable) != career_vt) {
            swofty::log("[ui.tile] hook #%u: %s: the adder result is unexpected (items %zu -> %zu); tile not added", call,
                        t.id.c_str(), before, after);
            break;
        }
        swofty::subclass(item, {{4, reinterpret_cast<void *>(mod_populate)}, {5, reinterpret_cast<void *>(mod_get_action)}});
        Ne_Make_StrFromCStr(&item->id, t.id.c_str(), 0, 0);
        {
            std::lock_guard<std::mutex> g(record_lock);
            auto title = frontend::tile_records::title_text(t);
            record_tiles[item] = RecordTile{title.text, t.route, title.localised};
        }
        c.r12 = index;
        c.r15 = index;
        added++;
        images += !t.image.empty();
    }
    auto world = frontend::tile_records::read(frontend::tile_records::Place::f1_world, {});
    if (!world.tiles.empty())
        if (auto n = world_lines.note())
            swofty::log("[ui.tile] f1_world: %zu tile record(s) not shown: the F1 World list builder is not hooked yet (the F1 "
                        "World screen's tile list site and list struct are not proven)%s",
                        world.tiles.size(), n.tail ? n.tail : "");
    if (images)
        if (auto n = image_lines.note())
            swofty::log("[ui.tile] home: %zu tile(s) name an image; tile images on home are not hooked yet (the per-tile image "
                        "VM leaf is not proven), they show without it%s",
                        images, n.tail ? n.tail : "");
    if (added || found.tiles.size())
        swofty::log("[ui.tile] hook #%u: %zu of %zu home tile record(s) added (%zu gated out, %zu refused, %zu over the cap)",
                    call, added, found.tiles.size(), found.gated_out, found.refused, found.capped);
}

SWOFTY_SITE(site_FeHome_AfterCareerCall, swofty::Context &c)
{
    unsigned call = ++tile.hook_calls;
    auto list = reinterpret_cast<TileList *>(c.r14);
    swofty::log("[core] hook #%u: list(r14)=0x%llx r12=0x%llx r15=0x%llx rsi=0x%llx rip=0x%llx", call, (unsigned long long)c.r14,
                (unsigned long long)c.r12, (unsigned long long)c.r15, (unsigned long long)c.rsi, (unsigned long long)c.rip);
    size_t before = 0, after = 0;
    HomeItem *career = frontend::tile_list::last_item(list, &before);
    if (!career) {
        swofty::log("[core] hook #%u: items vector does not end with a tile, nothing added", call);
        return;
    }
    auto career_vt = reinterpret_cast<void **>(career->vtable);
    uint8_t last = frontend::main_menu::add_tiles(list, uint8_t(c.r12), reinterpret_cast<const uint8_t *>(c.rsi));
    frontend::tile_list::last_item(list, &before);
    uint8_t index = uint8_t(last + 1);
    FeHome_Add_CareerTile(list, index);
    HomeItem *item = frontend::tile_list::last_item(list, &after);
    c.r12 = index;
    c.r15 = index;
    if (!item || after != before + 1 || reinterpret_cast<void **>(item->vtable) != career_vt) {
        swofty::log("[core] hook #%u: adder result unexpected (items %zu -> %zu, item %p, vt %p vs career %p); tile "
                    "left as a plain Career copy",
                    call, before, after, item, item ? static_cast<void *>(item->vtable) : nullptr, career_vt);
        return;
    }
    void **vt =
        swofty::subclass(item, {{4, reinterpret_cast<void *>(mod_populate)}, {5, reinterpret_cast<void *>(mod_get_action)}});
    if (vt != tile.mod_vt) {
        tile.mod_vt = vt;
        swofty::log("[core] hook #%u: built mod vtable %p from career vtable %p (slot 4 populate %p -> %p, slot 5 "
                    "getAction %p -> %p)",
                    call, vt, career_vt, career_vt[4], vt[4], career_vt[5], vt[5]);
    }
    NeStr *old_id = item->id;
    Ne_Make_StrFromCStr(&item->id, ROUTE, 0, 0);
    frontend::main_menu::style_settings(item);
    swofty::log("[core] hook #%u: added button_%u (items %zu -> %zu) item=%p, id %p -> %p '%s' (old id released by "
                "the game's string setter), r12 -> 0x%llx, r15 -> 0x%llx",
                call, (unsigned)index, before, after, item, old_id, item->id, ROUTE, (unsigned long long)c.r12,
                (unsigned long long)c.r15);
    add_record_tiles(c, list, career_vt, call);
}

#endif

SWOFTY_WRAP(FeHome_Handle_Command, uint64_t, FEHomeViewModel *vm, NeStr **cmd)
{
    static const uint32_t probe = perf::probe("core", "home command (addon settings)");
    if (vm && cmd && *cmd)
        frontend::main_menu::command(vm, *reinterpret_cast<const uint32_t *>(*cmd),
                                     *reinterpret_cast<const uint32_t *>(reinterpret_cast<const uint8_t *>(vm) + 0x3f0));
    {
        perf::Scope p(probe);
        settings_command(cmd && *cmd ? *reinterpret_cast<const uint32_t *>(*cmd) : 0x1505);
    }
    runtime::session_host_home(vm);
    runtime::testagent_home_vm(vm);
    return orig(vm, cmd);
}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("core: F1 THE MOVIE home tile", hide_movie_tile);
    runtime::when_code_ready("core: Braking Point home tile", hide_story_tile);
}

}

SWOFTY_MOD(frontend::check)
