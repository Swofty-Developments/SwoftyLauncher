#pragma once

#include "core/career/career_content_logic.hpp"
#include "core/runtime/loader/hook_set.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <string>

// What the career contents' registry (career_content.cpp), game hooks (content_hooks.cpp), addon table
// (content_api.cpp) and tiles and Addons row (content_tiles.cpp) share: the content slots and the lookups over
// them, the addresses bind_all() resolves, the hooks' originals and the last pressed tile's data group.
namespace career::content {

namespace mem = swofty::mem;
using swofty::game::Action;
using swofty::game::HomeItem;
using swofty::game::NeAllocator;
using swofty::game::NeStr;
using swofty::game::Tile;
using swofty::game::TileList;

constexpr uint32_t LOG_FIRST = 6;

using Route = Action *(HomeItem *, Action *);
using Command = void(void *, void *, void *);
using Gate = bool(const void *);
using Predicate = bool(void *, void *);

struct Slot {
    content::Desc desc;
    std::string owner;
    std::atomic<uint32_t> state{SWOFTY_CAREER_CONTENT_QUEUED};
    std::atomic<bool> resolved{false};
    std::atomic<int64_t> mode{-1};
    std::atomic<int64_t> group{-1};
    std::atomic<uint32_t> tiles{0}, presses{0}, gates{0}, slots{0};
    std::atomic<bool> in_row{false};
    std::string why;
    void **vt = nullptr;
    content::Retry retry;
    content::Desc given;
    std::string record;
    bool from_record = false;
};

struct Bound {
    uintptr_t site, route, gate, predicate_vt, select_vt, add_tile, construct, populate_2026, populate_2025, customisation,
        selected, setup_desc, make_str, set_string, vm_registry, list_child, set_bool;
};

extern Slot slots[content::MAX_CONTENTS];
extern Bound b;
// The career content set: the driver-career gate, the save-slot predicate and the content select command.
struct ContentHooks : runtime::loader::HookSet<> {
    runtime::loader::Original<Gate> driver_gate;
    runtime::loader::Original<Predicate> slot_predicate;
    runtime::loader::Original<Command> select_command;
};

// The career tile route set, shared with the career wizard library.
struct RouteHooks : runtime::loader::HookSet<> {
    runtime::loader::Original<Route> route_item;
};

extern ContentHooks hooks;
extern RouteHooks route;
extern std::mutex pressed_lock;
extern std::string pressed_group;
extern bool pressed_any;
extern const content::Access ACCESS;

void turn_off(Slot &s, const std::string &why);
size_t used();
void resolve_all(bool now, bool building = false);
bool ready(const Slot &s);
Slot *by_vtable(const void *item);
std::string suffix_of_group(const std::string &group);
int64_t mode_stat_of(const std::string &save);
void shown_from_records();
void resolve(Slot &s, bool now, bool building = false);
int add_slot(const content::Desc &desc, const std::string &who, const std::string &addon, const std::string *record);
uintptr_t game_manager_slot();
content::Loaded loaded_lists(uintptr_t slot, int64_t id);
bool bind_all(std::string &why);
Action *route_entry(HomeItem *item, Action *out);
bool gate_entry(const void *championship);
bool predicate_entry(void *self, void *championship);

inline const char *nestr(const NeStr *s)
{
    return s && mem::readable(s, content::NESTR_CHARS + 1) ? reinterpret_cast<const char *>(s) + content::NESTR_CHARS : nullptr;
}

void site_entry(SwoftyContext *c);
void command_entry(void *vm, void *cmd, void *data);

}
