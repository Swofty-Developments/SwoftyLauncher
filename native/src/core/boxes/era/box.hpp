#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

#include "core/apply/facts.hpp"
#include "core/boxes/node_handle.hpp"

namespace runtime::era {

constexpr const char *HOST = "swofty_f1";
constexpr const char *SELECTED_ADDON_TILE = "selected_addon_tile";
constexpr const char *ADDON_TILES_VISIBLE = "addon_tiles_visible";
constexpr const char *IMAGE_ARRAY = "#overrides.home_screen.tiles_solo_6x6";
// FeTile_Add_EraTile takes the tile index as a byte (the count of tiles added so far; the tiles themselves go into
// a growing vector and VM list), so the addon row holds up to 255 boxes, index 0..254.
constexpr size_t MAX_BOXES = 255;
constexpr uint32_t ONLY_ANY = 0, ONLY_GRAND_PRIX = 1, ONLY_TIME_TRIAL = 2;
constexpr size_t LIST_NAME_MAX = 63;
constexpr size_t TEXT_MAX = 255;

constexpr uint32_t djb2(const char *s)
{
    uint32_t h = 5381;
    while (*s)
        h = h * 33 + uint8_t(*s++);
    return h;
}

constexpr uint32_t ADDON_TILES = djb2("addon_tiles");
constexpr uint32_t ADDON_PRESS = djb2("addon_select_pressed");
constexpr uint32_t SELECT_PRESS = djb2("select_pressed");
static_assert(ADDON_TILES == 0xf2614e2b && ADDON_PRESS == 0x8fb68dff && SELECT_PRESS == 0x2b0357a);

struct Box {
    std::string entrant_list, title, image;
    int32_t order = 0;
    std::string owner;
    size_t seq = 0;
    std::string time_trial;
    uint32_t only = ONLY_ANY;
};

inline bool shown(const Box &box, bool time_trial)
{
    return box.only == ONLY_ANY || box.only == (time_trial ? ONLY_TIME_TRIAL : ONLY_GRAND_PRIX);
}

inline bool plain_name(const std::string &s)
{
    if (s.empty() || s.size() > LIST_NAME_MAX)
        return false;
    return std::all_of(s.begin(), s.end(), [](char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
    });
}

inline std::string check(Box &box, const std::vector<Box> &have)
{
    if (!plain_name(box.entrant_list))
        return "entrant_list must be 1 to 63 letters, digits or underscores";
    if (box.only > ONLY_TIME_TRIAL)
        return "only must be 0 (both pickers), 1 (Grand Prix) or 2 (Time Trial)";
    if (box.time_trial == box.entrant_list)
        box.time_trial.clear();
    if (!box.time_trial.empty() && !plain_name(box.time_trial))
        return "time_trial_entrant_list must be empty or 1 to 63 letters, digits or underscores";
    if (box.title.empty())
        box.title = "lng_" + box.entrant_list;
    if (box.title.size() > TEXT_MAX || box.image.size() > TEXT_MAX)
        return "title and image must be at most 255 characters";
    if (box.title.find('\0') != std::string::npos || box.image.find('\0') != std::string::npos)
        return "title and image must not hold NUL bytes";
    for (const auto &h : have)
        if (h.entrant_list == box.entrant_list)
            return "a box for this entrant list is already registered by " + h.owner;
    if (have.size() >= MAX_BOXES)
        return "the core holds at most 255 boxes";
    return {};
}

inline std::string image_of(const Box &box)
{
    return box.image.empty() ? std::string(IMAGE_ARRAY) + "[" + box.entrant_list + "]" : box.image;
}

inline bool time_trial_mode(std::string_view label) { return swofty_core::apply::facts::mode_of(label) == "time_trial"; }

struct FrontEndLeave {
    bool restore = false, clear_selected = false, forget_pick = false;
};

inline FrontEndLeave leaving_front_end(std::string_view mode_label, bool selected_is_box)
{
    std::string mode = swofty_core::apply::facts::mode_of(mode_label);
    if (mode != "driver_career" && mode != "my_team" && mode != "story" && mode != "f1_world")
        return {};
    return {true, selected_is_box, true};
}

inline const std::string &list_for(const Box &box, bool time_trial)
{
    return time_trial && !box.time_trial.empty() ? box.time_trial : box.entrant_list;
}

inline std::vector<Box> ordered(std::vector<Box> boxes)
{
    std::stable_sort(boxes.begin(), boxes.end(),
                     [](const Box &a, const Box &b) { return a.order != b.order ? a.order < b.order : a.seq < b.seq; });
    return boxes;
}

// clear (FeTile_Clear_EraTileSet_Thunk in game) unlinks and releases both handles from the nodes they still
// hold, whichever view model those belong to, before the list handle is linked to this build's node.
template <class Set, class Clear> void relink(Set &set, uint8_t *list, Clear clear)
{
    if (set.list.node || set.selected.node)
        clear(set);
    set = {};
    link(set.list, list);
}

inline std::string press_refusal(bool same_view_model, bool nodes, size_t vector_tiles, size_t stock, size_t boxes)
{
    if (!same_view_model)
        return "the picker that raised it is not the one the addon row was built for";
    if (!nodes || !boxes)
        return "the addon row has no boxes";
    if (vector_tiles < stock + boxes)
        return "the tile vector holds " + std::to_string(vector_tiles) + " tiles, fewer than the " + std::to_string(stock) +
               " stock and " + std::to_string(boxes) + " addon tiles of the last build";
    return {};
}

}
