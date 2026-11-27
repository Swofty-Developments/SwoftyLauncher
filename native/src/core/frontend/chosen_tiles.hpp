#pragma once
#include <algorithm>
#include <cstddef>
#include <set>
#include <string>
#include <vector>

#include <swofty_api.h>

#include "core/frontend/tile_records.hpp"

namespace frontend::chosen_tiles {

// The record tiles a builder has chosen and not yet added: what the ui.career_content.tiles and ui.car_category.tiles
// payload `list` points at. A handler edits tiles (drop, reorder); the builder settles the result before it adds.
struct Chosen {
    std::vector<frontend::tile_records::Tile> &tiles;
    size_t cap;
    frontend::tile_records::Place place = frontend::tile_records::Place::home;
    bool need_route = true;
};

// What a handler may not do: show more tiles than the screen has room for, or two tiles of one id. A route a handler
// set that names a ui.route record by its id becomes the record's key, as it does for a record tile; an id no record
// has is passed as written and reported in `lines`, once per tile.
inline size_t settle(std::vector<frontend::tile_records::Tile> &tiles, size_t cap,
                     const frontend::tile_records::RouteExists &route_exists = {}, std::vector<std::string> *lines = nullptr)
{
    if (route_exists)
        for (auto &t : tiles) {
            bool unknown = false;
            t.route = frontend::tile_records::game_route(t.route, route_exists, &unknown);
            if (unknown && !t.route_told) {
                t.route_told = true;
                if (lines)
                    lines->push_back(frontend::tile_records::unknown_route_line(t.id, t.route));
            }
        }
    std::set<std::string> seen;
    std::vector<frontend::tile_records::Tile> kept;
    for (auto &t : tiles)
        if (kept.size() < cap && seen.insert(t.id).second)
            kept.push_back(std::move(t));
    size_t dropped = tiles.size() - kept.size();
    tiles = std::move(kept);
    return dropped;
}

// The SwoftyTileList table of a payload (`tiles`) over a Chosen: count, get, remove, move, insert, with the bounds and
// the cap checked here so a handler can never hand the add step more than settle would keep. The table and its Chosen
// live on the caller's stack for the duration of the fire.
class ChosenTable {
public:
    explicit ChosenTable(Chosen &c) : chosen_(c)
    {
        table_ = {sizeof(SwoftyTileList), SWOFTY_TILE_LIST_VERSION, this, &count, &get, &remove, &move, &insert};
    }

    const SwoftyTileList *table() const { return &table_; }

private:
    static ChosenTable &of(void *self) { return *static_cast<ChosenTable *>(self); }
    static bool in(const Chosen &c, int i) { return i >= 0 && size_t(i) < c.tiles.size(); }

    static int count(void *self) { return int(of(self).chosen_.tiles.size()); }

    static int get(void *self, int i, SwoftyTileView *out)
    {
        auto &c = of(self).chosen_;
        if (!out || !in(c, i))
            return SWOFTY_ASSET_INVALID;
        const auto &t = c.tiles[size_t(i)];
        out->id = t.id.c_str();
        out->title = t.title.c_str();
        out->route = t.route.c_str();
        out->image = t.image.c_str();
        out->place = frontend::tile_records::place_name(t.place);
        out->literal = t.literal ? 1 : 0;
        return SWOFTY_ASSET_OK;
    }

    static int remove(void *self, int i)
    {
        auto &c = of(self).chosen_;
        if (!in(c, i))
            return SWOFTY_ASSET_INVALID;
        c.tiles.erase(c.tiles.begin() + i);
        return SWOFTY_ASSET_OK;
    }

    static int move(void *self, int from, int to)
    {
        auto &c = of(self).chosen_;
        if (!in(c, from) || !in(c, to))
            return SWOFTY_ASSET_INVALID;
        auto t = std::move(c.tiles[size_t(from)]);
        c.tiles.erase(c.tiles.begin() + from);
        c.tiles.insert(c.tiles.begin() + to, std::move(t));
        return SWOFTY_ASSET_OK;
    }

    static int insert(void *self, int at, const SwoftyTileSpec *spec)
    {
        auto &c = of(self).chosen_;
        if (!spec || !spec->id || !*spec->id || !spec->title || !*spec->title || at < 0 || size_t(at) > c.tiles.size())
            return SWOFTY_ASSET_INVALID;
        if (c.need_route && (!spec->route || !*spec->route))
            return SWOFTY_ASSET_INVALID;
        if (c.tiles.size() >= c.cap)
            return SWOFTY_ASSET_INVALID;
        for (const auto &t : c.tiles)
            if (t.id == spec->id)
                return SWOFTY_ASSET_INVALID;
        frontend::tile_records::Tile t;
        t.id = spec->id;
        t.place = c.place;
        t.title = spec->title;
        t.literal = spec->literal != 0;
        t.image = spec->image ? spec->image : "";
        t.route = spec->route ? spec->route : "";
        c.tiles.insert(c.tiles.begin() + at, std::move(t));
        return SWOFTY_ASSET_OK;
    }

    Chosen &chosen_;
    SwoftyTileList table_{};
};

}
