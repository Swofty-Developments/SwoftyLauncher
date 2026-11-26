#pragma once
#include <cstddef>
#include <cstdint>
#include <optional>

#include "core/frontend/tile_records.hpp"

namespace frontend::f1_world {

inline constexpr uint8_t MENU_KIND = 3, SUB_KIND = 0;
inline constexpr size_t ENTRY_SIZE = 0x10;
inline constexpr size_t MENU_KIND_SLOT = 0x2f0, SUB_KIND_SLOT = 0x2f8;

inline bool is_world_list(uint8_t menu_kind, uint8_t sub_kind) { return menu_kind == MENU_KIND && sub_kind == SUB_KIND; }

inline std::optional<size_t> items_in(uintptr_t begin, uintptr_t end)
{
    if (!begin || end < begin || (end - begin) % ENTRY_SIZE)
        return std::nullopt;
    return (end - begin) / ENTRY_SIZE;
}

inline std::optional<uint8_t> next_index(size_t items)
{
    if (items + 1 > tile_records::TILE_CAP)
        return std::nullopt;
    return uint8_t(items + 1);
}

inline size_t room_for(size_t items) { return items >= tile_records::TILE_CAP ? 0 : tile_records::TILE_CAP - items; }

}
