#pragma once

#include "files.hpp"

#include <cstdint>
#include <string>

// Where the core builds an archive cache of its own (the team logo archives): swofty/addons/<id>/ in the game folder
// while the game can write there, otherwise addon-storage/<id>/ in the launcher's data folder, which the game's file
// route reaches through ../ (files::route_from_game). The game-folder path stays the first choice so an existing setup
// keeps its cache. Pure: the caller gives the probe, so the host test runs without Windows.

namespace swofty_core::apply::cache_place {

struct Place {
    std::wstring dir;
    std::string away;
    bool moved = false;
    uint32_t refused = 0;
    std::string note;
};

template <class Refuses>
Place choose(const std::wstring &game_root, const std::wstring &swofty_root, const std::wstring &data_root,
             const std::string &id, std::size_t longest, Refuses &&refuses)
{
    Place p;
    std::wstring wide_id(id.begin(), id.end());
    p.dir = swofty_root + L"addons\\" + wide_id + L"\\";
    p.refused = refuses(p.dir);
    if (!p.refused)
        return p;
    std::string shown = "swofty/addons/" + id + "/";
    std::string code = std::to_string(p.refused);
    if (data_root.empty()) {
        p.note = "the game can't create files in " + shown + " (error " + code + ") and the launcher's data folder is not known";
        return p;
    }
    std::wstring other = data_root;
    if (other.back() != L'\\' && other.back() != L'/')
        other += L'\\';
    other += L"addon-storage\\" + wide_id + L"\\";
    std::string why;
    std::string route = files::route_from_game(game_root, other, longest, why);
    if (route.empty()) {
        p.note = "the game can't create files in " + shown + " (error " + code + ") and the data folder can't be routed: " + why;
        return p;
    }
    if (uint32_t e = refuses(other)) {
        p.note = "the game can't create files in " + shown + " (error " + code + ") or in the data folder (error " +
                 std::to_string(e) + ")";
        return p;
    }
    p.dir = other;
    p.away = route;
    p.moved = true;
    return p;
}

}
