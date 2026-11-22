#pragma once

#include "core/apply/content/box_assets_lower.hpp"

// Lowers a track.sponsors record: a box's own trackside sponsor boards. A thin alias of content.box_assets with kind
// sponsors (box_assets_lower.hpp): one archive per record, reached through a game path of its own and listed first in
// the asset groups of every car the box's package names, so a session of any other box never loads it.

namespace swofty_core::apply::content {

inline constexpr const char *SPONSORS_FOLDER = "2025_asset_groups/swofty_sponsors/";
inline constexpr const char *SPONSORS_ROOT = "eaid://environment_package/core_assets/shared_textures/sponsors/";

inline bool box_sponsors_enabled(const char *env) { return garage_art_enabled(env); }

inline constexpr const char *SPONSORS_FENCE_LINE =
    "track.sponsors: left off; the stock sponsor surfaces are streamed (3 fragments) like the garage textures whose "
    "stock streamed copy crashed the texture stream in 1.7.8 to 1.7.10. SWOFTY_BOX_SPONSORS=1 turns it on";

inline std::string sponsors_game_path(const std::string &id)
{
    std::string s;
    for (unsigned char c : id)
        s += std::isalnum(c) ? char(std::tolower(c)) : '_';
    return std::string(SPONSORS_FOLDER) + s + ".erp";
}

inline void lower_sponsors(const std::string &id, const Value &rec, const Lookup &lookup, const Read &read, Plan &plan)
{
    BoxSpec s;
    s.tag = "track.sponsors";
    s.kind = "sponsors";
    if (!rec["package"].is_string() || !rec["textures"].is_string() || rec["textures"].as_string().empty()) {
        plan.owner = addon_of(id);
        plan.lines.push_back("track.sponsors " + id + ": it needs a package and a textures folder; nothing applied");
        plan.refused++;
        return;
    }
    s.package = std::string(rec["package"].as_string());
    s.folder = std::string(rec["textures"].as_string());
    s.root = rec["root"].is_string() ? std::string(rec["root"].as_string()) : std::string(SPONSORS_ROOT);
    s.srgb = !rec["srgb"].is_bool() || rec["srgb"].as_bool();
    s.game_path = sponsors_game_path(id);
    lower_box(id, std::move(s), lookup, read, plan);
}

}
