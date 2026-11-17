#pragma once
#include <swofty/game.hpp>

#include <cstddef>
#include <cstdint>

namespace tile_list {

inline swofty::game::HomeItem *last_item(swofty::game::TileList *list, size_t *count = nullptr)
{
    auto begin = reinterpret_cast<uintptr_t>(list->items_begin), end = reinterpret_cast<uintptr_t>(list->items_end);
    if (!begin || end <= begin || (end - begin) % sizeof(swofty::game::TileEntry))
        return nullptr;
    if (count)
        *count = list->items_end - list->items_begin;
    swofty::game::Tile *tile = list->items_end[-1].tile;
    return tile ? tile->item : nullptr;
}

inline void set_string(void *reg, void *parent, const char *name, const char *text)
{
    swofty::game::NeStr *s = nullptr;
    swofty::game::Ne_Make_StrFromCStr(&s, text, 0, 0);
    swofty::game::Vm_Set_String(reg, parent, name, &s, 1);
    swofty::game::Ne_Make_StrFromCStr(&s, nullptr, 0, 0);
}

inline swofty::game::Action *route_action(swofty::game::Action *out, const char *route)
{
    out->type = 1;
    out->str = nullptr;
    swofty::game::Ne_Make_StrFromCStr(&out->str, route, 0, 0);
    return out;
}

}
