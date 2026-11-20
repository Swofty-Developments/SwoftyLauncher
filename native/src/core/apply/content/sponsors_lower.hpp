#pragma once

#include "core/apply/content/garage_lower.hpp"

// Lowers a track.sponsors record: a box's own trackside sponsor boards. The textures go the garage art's way
// (garage_build.hpp, apply_garages): one archive per record, reached through a game path of its own and listed first
// in the asset groups of every car the box's package names, so a session of any other box never loads it.

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
    plan.owner = addon_of(id);
    auto refuse = [&](const std::string &why) {
        plan.lines.push_back("track.sponsors " + id + ": " + why + "; nothing applied");
        plan.refused++;
    };
    if (!rec["package"].is_string() || !rec["textures"].is_string() || rec["textures"].as_string().empty())
        return refuse("it needs a package and a textures folder");
    std::string package(rec["package"].as_string());
    const Value *pkg = lookup("content.package", package);
    if (!pkg)
        return refuse("package " + package + " is not a record");
    GarageArt g;
    g.registry = "track.sponsors";
    g.id = id;
    g.addon = addon_of(id);
    g.package = package;
    g.folder = std::string(rec["textures"].as_string());
    if (g.folder.back() != '/')
        g.folder += '/';
    g.root = rec["root"].is_string() ? std::string(rec["root"].as_string()) : std::string(SPONSORS_ROOT);
    g.srgb = !rec["srgb"].is_bool() || rec["srgb"].as_bool();
    g.game_path = sponsors_game_path(id);
    const Value &teams = (*pkg)["teams"];
    for (std::size_t t = 0; t < teams.size(); t++)
        if (teams[t]["vehicle"].is_string()) {
            std::string vid(teams[t]["vehicle"].as_string());
            if (std::find(g.vehicles.begin(), g.vehicles.end(), vid) == g.vehicles.end())
                g.vehicles.push_back(vid);
        }
    if (g.vehicles.empty())
        return refuse("package " + package + " names no car");
    for (auto &vid : g.vehicles) {
        CarPaths car = car_paths(record_label(lookup, "vehicle", vid), read);
        if (!car.ok()) {
            plan.lines.push_back("track.sponsors " + id + " on " + vid + ": no asset group: " + car.why);
            continue;
        }
        if (std::find(g.groups.begin(), g.groups.end(), car.group) == g.groups.end())
            g.groups.push_back(car.group);
    }
    if (g.groups.empty())
        return refuse("none of the cars of " + package + " has an asset group");
    plan.lines.push_back("track.sponsors " + id + ": boards of " + package + " for " + std::to_string(g.groups.size()) +
                         " car group(s) of " + std::to_string(g.vehicles.size()) + " car(s)");
    plan.garages.push_back(std::move(g));
}

}
