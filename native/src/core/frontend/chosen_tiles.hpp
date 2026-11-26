#pragma once
#include <algorithm>
#include <cstddef>
#include <set>
#include <string>
#include <vector>

#include "core/frontend/tile_records.hpp"

namespace frontend::chosen_tiles {

// The record tiles a builder has chosen and not yet added: what the ui.career_content.tiles and ui.car_category.tiles
// payload `list` points at. A handler edits tiles (drop, reorder); the builder settles the result before it adds.
struct Chosen {
    std::vector<frontend::tile_records::Tile> &tiles;
    size_t cap;
};

// What a handler may not do: show more tiles than the screen has room for, or two tiles of one id.
inline size_t settle(std::vector<frontend::tile_records::Tile> &tiles, size_t cap)
{
    std::set<std::string> seen;
    std::vector<frontend::tile_records::Tile> kept;
    for (auto &t : tiles)
        if (kept.size() < cap && seen.insert(t.id).second)
            kept.push_back(std::move(t));
    size_t dropped = tiles.size() - kept.size();
    tiles = std::move(kept);
    return dropped;
}

}
