#pragma once

#include "core/career/career_content_logic.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <string>

// What the career contents' registry and hooks (career_content.cpp) and their tiles and Addons row
// (content_tiles.cpp) share: the content slots and the lookups over them, the addresses bind_all() resolves, and
// the tile site and content select command handler the registry offers as mixins.
namespace career_content {

namespace content = runtime::content;
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
    uintptr_t site, route, gate, predicate_vt, select_vt, add_tile, construct, populate_2026, populate_2025,
        customisation, selected, setup_desc, make_str, set_string, vm_registry, list_child, set_bool;
};

extern Slot slots[content::MAX_CONTENTS];
extern Bound b;
extern Command *command_orig;

void turn_off(Slot &s, const std::string &why);
size_t used();
void resolve_all(bool now, bool building = false);
bool ready(const Slot &s);
Slot *by_vtable(const void *item);
void shown_from_records();

inline const char *nestr(const NeStr *s)
{
    return s && mem::readable(s, content::NESTR_CHARS + 1) ? reinterpret_cast<const char *>(s) + content::NESTR_CHARS
                                                           : nullptr;
}

void site_entry(SwoftyContext *c);
void command_entry(void *vm, void *cmd, void *data);

}
