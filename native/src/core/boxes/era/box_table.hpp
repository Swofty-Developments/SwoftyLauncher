#pragma once

#include <cstddef>
#include <deque>
#include <string>
#include <vector>

#include <swofty_api.h>

#include "core/boxes/era/box.hpp"

namespace runtime::era {

// The SwoftyTileList table of ui.era.tiles: the addon boxes the picker is about to add to its addon row, as the core
// holds them (ordered, before availability checks). The game's stock era tiles are the vector's first entries, each a
// 16-byte {allocator, tile*} pair whose tile carries its title and id in view-model nodes this core does not read, and
// the addon row's tiles are added after this event, so the table lists only the core's own boxes: remove and move edit
// that list before FeTile_Add_EraTile sees it. Indices count the boxes this picker shows (Box::only against the
// picker's mode); a hidden box keeps its place. insert is not offered: era boxes are added through era.box records.
class BoxTable {
public:
    BoxTable(std::vector<Box> &boxes, bool time_trial) : boxes_(boxes), time_trial_(time_trial)
    {
        table_ = {sizeof(SwoftyTileList), SWOFTY_TILE_LIST_VERSION, this, &count, &get, &remove, &move, nullptr};
    }

    const SwoftyTileList *table() const { return &table_; }

private:
    static BoxTable &of(void *self) { return *static_cast<BoxTable *>(self); }

    std::vector<size_t> shown_indices() const
    {
        std::vector<size_t> out;
        for (size_t i = 0; i < boxes_.size(); i++)
            if (shown(boxes_[i], time_trial_))
                out.push_back(i);
        return out;
    }

    static int count(void *self) { return int(of(self).shown_indices().size()); }

    static int get(void *self, int i, SwoftyTileView *out)
    {
        auto &t = of(self);
        auto idx = t.shown_indices();
        if (!out || i < 0 || size_t(i) >= idx.size())
            return SWOFTY_ASSET_INVALID;
        const Box &b = t.boxes_[idx[size_t(i)]];
        t.text_.push_back(list_for(b, t.time_trial_));
        const char *route = t.text_.back().c_str();
        t.text_.push_back(image_of(b));
        out->id = b.entrant_list.c_str();
        out->title = b.title.c_str();
        out->route = route;
        out->image = t.text_.back().c_str();
        out->place = "era";
        out->literal = 0;
        return SWOFTY_ASSET_OK;
    }

    static int remove(void *self, int i)
    {
        auto &t = of(self);
        auto idx = t.shown_indices();
        if (i < 0 || size_t(i) >= idx.size())
            return SWOFTY_ASSET_INVALID;
        t.boxes_.erase(t.boxes_.begin() + std::ptrdiff_t(idx[size_t(i)]));
        t.text_.clear();
        return SWOFTY_ASSET_OK;
    }

    static int move(void *self, int from, int to)
    {
        auto &t = of(self);
        auto idx = t.shown_indices();
        if (from < 0 || to < 0 || size_t(from) >= idx.size() || size_t(to) >= idx.size())
            return SWOFTY_ASSET_INVALID;
        std::vector<Box> order;
        for (size_t i : idx)
            order.push_back(std::move(t.boxes_[i]));
        Box moved = std::move(order[size_t(from)]);
        order.erase(order.begin() + from);
        order.insert(order.begin() + to, std::move(moved));
        for (size_t k = 0; k < idx.size(); k++)
            t.boxes_[idx[k]] = std::move(order[k]);
        t.text_.clear();
        return SWOFTY_ASSET_OK;
    }

    std::vector<Box> &boxes_;
    bool time_trial_;
    std::deque<std::string> text_;
    SwoftyTileList table_{};
};

}
