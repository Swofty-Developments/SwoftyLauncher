#pragma once

#include "core/apply/content/garage_lower.hpp"

#include <algorithm>
#include <string>
#include <vector>

// Lowers a content.box_assets record: stock-named resources that only one box's sessions load. The archive (a folder
// of .dds and .png files the core packs the garage art's way, or a ready .erp of the addon) is reached through a game
// path of its own and listed first in the asset groups of the box's cars (all of them, or one team entry's), so a
// session of any other box never loads it. The game keeps the first copy of a resource name it loads (resource map
// find-or-create, re/db/subsystems/assets.md), so the archive only wins where the box's car groups load before the
// stock archive holding the same name; docs/std/systems/box-assets.md lists what is known per kind.
// track.sponsors (sponsors_lower.hpp) is this route with kind sponsors.
// Kind scene lists a ready archive first in stock scene groups instead (the grid cutscene, a garage scene): those groups
// are shared by every session, so the core keeps two ERP lists for each and switches the group to the archive's list
// while the box is the session's box (box rules: a box press or a career season), and back to the stock list after.

namespace swofty_core::apply::content {

inline constexpr const char *BOX_ASSETS_FOLDER = "2025_asset_groups/swofty_box_assets/";
inline constexpr const char *BOX_ASSET_KINDS[] = {"garage", "sponsors", "marshals", "crowd", "principal", "scene", "other"};

inline bool box_assets_enabled(const char *env) { return garage_art_enabled(env); }

inline constexpr const char *BOX_ASSETS_FENCE_LINE =
    "content.box_assets: left off; stock textures are streamed (3 fragments) like the garage textures whose stock streamed "
    "copy crashed the texture stream in 1.7.8 to 1.7.10 (Ne_Crt_Memcpy). SWOFTY_BOX_ASSETS=1 turns it on";

inline std::string box_assets_game_path(const std::string &id)
{
    std::string s;
    for (unsigned char c : id)
        s += std::isalnum(c) ? char(std::tolower(c)) : '_';
    return std::string(BOX_ASSETS_FOLDER) + s + ".erp";
}

inline bool box_asset_kind(const std::string &kind)
{
    for (const char *k : BOX_ASSET_KINDS)
        if (kind == k)
            return true;
    return false;
}

// The resource name a textures folder's file paths are appended to when the record names no root: the stock root of
// the garage art and of the sponsor boards, else eaid:// itself, so the folder mirrors the whole resource name.
inline std::string box_assets_root(const std::string &kind)
{
    if (kind == "garage")
        return "eaid://environment_package/core_assets/objects/textures/";
    if (kind == "sponsors")
        return "eaid://environment_package/core_assets/shared_textures/sponsors/";
    return "eaid://";
}

inline bool box_name_allowed(const std::string &name, const std::vector<std::string> &prefixes)
{
    if (prefixes.empty())
        return true;
    for (auto &p : prefixes)
        if (name.compare(0, p.size(), p) == 0)
            return true;
    return false;
}

// What a ready archive holds: its resources, the ones outside the record's prefixes, and its texture surfaces with a
// streaming descriptor (a third fragment), the fragment the 1.7.8 crash copied.
// appliers.cpp reads the entries from the archive's index (erp_file.hpp).
struct BoxArchiveEntry {
    std::string name, type;
    std::size_t fragments = 0;
};

struct BoxArchiveCheck {
    std::size_t resources = 0, outside = 0, streamed = 0;
    std::string first_outside;
};

inline BoxArchiveCheck check_box_archive(const std::vector<BoxArchiveEntry> &entries, const std::vector<std::string> &prefixes)
{
    BoxArchiveCheck c;
    for (auto &e : entries) {
        c.resources++;
        if (!box_name_allowed(e.name, prefixes)) {
            if (!c.outside)
                c.first_outside = e.name;
            c.outside++;
        }
        if (e.type == "GfxSurfaceRes" && e.fragments >= 3)
            c.streamed++;
    }
    return c;
}

// One box archive request, whichever registry wrote it.
struct BoxSpec {
    std::string tag, kind, package, entry, folder, root, archive, game_path;
    bool srgb = true;
    std::vector<std::string> prefixes, scene_groups;
};

inline void lower_box(const std::string &id, BoxSpec s, const Lookup &lookup, const Read &read, Plan &plan)
{
    plan.owner = addon_of(id);
    auto refuse = [&](const std::string &why) {
        plan.lines.push_back(s.tag + " " + id + ": " + why + "; nothing applied");
        plan.refused++;
    };
    const Value *pkg = lookup("content.package", s.package);
    if (!pkg)
        return refuse("package " + s.package + " is not a record");
    GarageArt g;
    g.registry = s.tag;
    g.kind = s.kind;
    g.id = id;
    g.addon = addon_of(id);
    g.package = s.package;
    g.team = s.entry;
    g.folder = s.folder;
    if (!g.folder.empty() && g.folder.back() != '/')
        g.folder += '/';
    g.archive = s.archive;
    g.root = s.root;
    g.srgb = s.srgb;
    g.prefixes = s.prefixes;
    g.game_path = s.game_path;
    if (s.kind == "scene") {
        if (!(*pkg)["entrant_list"].is_string() || (*pkg)["entrant_list"].as_string().empty())
            return refuse("package " + s.package + " has no entrant_list, so no box session can switch its scene groups");
        g.box = std::string((*pkg)["entrant_list"].as_string());
        g.groups = s.scene_groups;
        plan.lines.push_back(s.tag + " " + id + ": scene of " + s.package + " (box " + g.box + ") for " +
                             std::to_string(g.groups.size()) + " scene group(s), archive " + g.archive);
        plan.garages.push_back(std::move(g));
        return;
    }
    const Value &teams = (*pkg)["teams"];
    for (std::size_t t = 0; t < teams.size(); t++) {
        if (!teams[t]["vehicle"].is_string())
            continue;
        if (!s.entry.empty() && !(teams[t]["key"].is_string() && teams[t]["key"].as_string() == s.entry))
            continue;
        std::string vid(teams[t]["vehicle"].as_string());
        if (std::find(g.vehicles.begin(), g.vehicles.end(), vid) == g.vehicles.end())
            g.vehicles.push_back(vid);
    }
    if (g.vehicles.empty())
        return refuse(s.entry.empty() ? "package " + s.package + " names no car"
                                      : "package " + s.package + " has no team entry " + s.entry + " with a car");
    for (auto &vid : g.vehicles) {
        CarPaths car = car_paths(record_label(lookup, "vehicle", vid), read);
        if (!car.ok()) {
            plan.lines.push_back(s.tag + " " + id + " on " + vid + ": no asset group: " + car.why);
            continue;
        }
        if (std::find(g.groups.begin(), g.groups.end(), car.group) == g.groups.end())
            g.groups.push_back(car.group);
    }
    if (g.groups.empty())
        return refuse("none of the cars of " + s.package + " has an asset group");
    plan.lines.push_back(s.tag + " " + id + ": " + s.kind + " of " + s.package +
                         (s.entry.empty() ? "" : " (entry " + s.entry + ")") + " for " + std::to_string(g.groups.size()) +
                         " car group(s) of " + std::to_string(g.vehicles.size()) + " car(s), " +
                         (g.archive.empty() ? "folder " + g.folder : "archive " + g.archive));
    plan.garages.push_back(std::move(g));
}

inline bool box_relative_path(const std::string &p)
{
    return !p.empty() && p[0] != '/' && p.find('\\') == std::string::npos && p.find(':') == std::string::npos &&
           p.find("..") == std::string::npos;
}

inline void lower_box_assets(const std::string &id, const Value &rec, const Lookup &lookup, const Read &read, Plan &plan)
{
    BoxSpec s;
    s.tag = "content.box_assets";
    auto refuse = [&](const std::string &why) {
        plan.owner = addon_of(id);
        plan.lines.push_back(s.tag + " " + id + ": " + why + "; nothing applied");
        plan.refused++;
    };
    auto text = [&](const char *key) { return rec[key].is_string() ? std::string(rec[key].as_string()) : std::string(); };
    s.package = text("package");
    s.kind = rec["kind"].is_string() ? text("kind") : "other";
    s.entry = text("entry");
    s.folder = text("textures");
    s.archive = text("archive");
    if (s.package.empty())
        return refuse("it needs a package");
    if (!box_asset_kind(s.kind))
        return refuse("kind " + s.kind + " is not one of garage, sponsors, marshals, crowd, principal, scene, other");
    if (s.folder.empty() == s.archive.empty())
        return refuse("it needs one of a textures folder and an archive, not both");
    if (!box_relative_path(s.folder.empty() ? s.archive : s.folder))
        return refuse("the textures folder or archive must be a path inside the addon's folder");
    const Value &res = rec["resources"];
    for (std::size_t i = 0; i < res.size(); i++) {
        if (!res[i].is_string() || res[i].as_string().rfind("eaid://", 0) != 0)
            return refuse("every resources entry must be an eaid:// name prefix");
        s.prefixes.push_back(std::string(res[i].as_string()));
    }
    if (!s.archive.empty() && s.prefixes.empty())
        return refuse("an archive needs the resources (eaid:// prefixes) it may hold");
    if (!s.archive.empty() && (s.archive.size() < 5 || s.archive.compare(s.archive.size() - 4, 4, ".erp") != 0))
        return refuse("the archive " + s.archive + " is not an .erp file");
    const Value &groups = rec["groups"];
    for (std::size_t i = 0; i < groups.size(); i++) {
        std::string gr = groups[i].is_string() ? std::string(groups[i].as_string()) : std::string();
        if (gr.size() < 12 || gr.compare(gr.size() - 11, 11, ".assetgroup") != 0 || gr.find("..") != std::string::npos)
            return refuse("every groups entry must be an asset group name ending in .assetgroup");
        s.scene_groups.push_back(gr);
    }
    if (s.kind == "scene" && (s.archive.empty() || s.scene_groups.empty()))
        return refuse("kind scene needs a ready archive and the scene groups it goes first in");
    if (s.kind != "scene" && !s.scene_groups.empty())
        return refuse("groups are for kind scene only; the other kinds go to the box's car groups");
    s.root = text("root").empty() ? box_assets_root(s.kind) : text("root");
    s.srgb = !rec["srgb"].is_bool() || rec["srgb"].as_bool();
    s.game_path = box_assets_game_path(id);
    lower_box(id, std::move(s), lookup, read, plan);
}

}
