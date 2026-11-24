// SPDX-License-Identifier: MIT
#pragma once
#include <swofty/events.hpp>
#include <swofty/registry.hpp>
#include <swofty/systems/forward.hpp>
#include <swofty/systems/sessions.hpp>

#include <atomic>
#include <optional>
#include <string>
#include <string_view>

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

// Grand Prix and Time Trial content: a package is one season build (an entrant list, its teams, cars
// and drivers, and the box's rules); an era box is the tile in the era picker that opens it.

namespace swofty::content {

using Result = swofty::Result;

struct Seat {
    Ref<drivers::Identity> driver;
    int number = 0;
    Ref<drivers::Look> look;
};
SWOFTY_RECORD(Seat, driver, number, look)

struct Team {
    std::string key;
    Ref<teams::Identity> team;
    Ref<cars::Vehicle> vehicle;
    Ref<sounds::EngineSound> engine_sound;
    Ref<liveries::Livery> livery;
    std::vector<Seat> drivers;
};
SWOFTY_RECORD(Team, key, team, vehicle, engine_sound, livery, drivers)

struct WorldBranding {                             // content.package 1.7: a game_setup.branding row of the box's season
    std::string from;                              // the stock row it copies; empty: f1_26
    std::string timing_tower_logo;                 // timing_tower_logo_texture_path, a timing_tower_logos slice key
    std::optional<bool> broadcast_show_f1_logo;
};
SWOFTY_RECORD(WorldBranding, from, timing_tower_logo, broadcast_show_f1_logo)

struct World {                                     // content.package 1.7: the box's own world, [FILENAME]_<name>
    std::string name;                              // "e1950": environment_package/tracks/<code>/wep/<code>_e1950.erp
    std::string season;                            // shared by the boxes that name it; empty: name
    std::string from;                              // the stock track layer row it copies; empty: f1_26 (myteam_26 for 12 teams)
    std::optional<WorldBranding> branding;
};
SWOFTY_RECORD(World, name, season, from, branding)

struct Package {
    static constexpr Version version{1, 3};
    Text title;
    std::string entrant_list;
    int season = 0;
    Ref<career::Calendar> calendar;
    Keyed<Team, &Team::key> teams;
    sessions::Rules rules;
    std::string setup_mode = "swofty_f1";
    std::string leaderboard = "box";
    bool virtual_list = false;
    std::optional<World> world;                    // content.package 1.7, with SWOFTY_BOX_OWN_WORLDS=1
    StatId stat_id = StatId::automatic;
    json::Value extra;
};
SWOFTY_RECORD(Package, title, entrant_list, season, calendar, teams, rules, setup_mode, leaderboard, virtual_list,
              world, stat_id)

struct TimeTrial {
    std::string entrant_list;
};
SWOFTY_RECORD(TimeTrial, entrant_list)

enum class EraOnly { grand_prix, time_trial };
SWOFTY_ENUM(EraOnly, grand_prix, time_trial)

struct EraBox {
    static constexpr Version version{1, 2};
    Ref<Package> package;
    Text title;
    std::string image;
    int order = 0;
    TimeTrial time_trial;
    std::optional<EraOnly> only;
    json::Value extra;
};
SWOFTY_RECORD(EraBox, package, title, image, order, time_trial, only)

// One box's own UI textures (ui.per_package 1.0, experimental): textures named like stock ui_package resources (a
// folder of .dds / .png files below root, or resources of an archive in the addon). While the box is the pressed one,
// the core routes each listed ui_package archive to its own copy with those resources swapped in; leaving the box
// puts the game's archives back. Archives the game keeps loaded from boot (persistent.erp, flow_persistent.erp,
// fonts) are refused: the game never opens them again, so a copy would not be read.
struct PackageUi {
    static constexpr Version version{1, 0};
    Ref<Package> package;
    Path textures;
    std::string root = "eaid://ui_package/art/";
    bool srgb = true;
    std::vector<std::string> archives;
    json::Value extra;
};
SWOFTY_RECORD(PackageUi, package, textures, root, srgb, archives)
SWOFTY_RECORD_REGISTRY(PackageUi, "ui.per_package")

enum class BoxAssetKind { garage, sponsors, marshals, crowd, principal, scene, world, other };
SWOFTY_ENUM(BoxAssetKind, garage, sponsors, marshals, crowd, principal, scene, world, other)

// Stock-named resources only one box's sessions load: a folder of .dds and .png files the core packs (the
// teams::Garage way), or a ready .erp of the addon. The archive is listed first in the asset groups of the box's
// cars (every car, or one team entry's), so a session of any other box never loads it. The game keeps the first
// copy of a name it loads, so it only shows where the cars load before the stock archive (docs: box-assets).
struct BoxAssets {
    static constexpr Version version{1, 2};
    Ref<Package> package;
    BoxAssetKind kind = BoxAssetKind::other;
    std::string entry;                  // the key of one team entry of the package; empty: every car of the box
    Path textures;                      // a folder of .dds and .png files, at their resource names below root
    std::string root;                   // empty: the kind's stock root (garage, sponsors) or eaid:// itself
    bool srgb = true;                   // a .png is encoded as sRGB BC7
    Path archive;                       // or a ready .erp archive of the addon
    std::vector<std::string> resources; // the eaid:// name prefixes it may hold (required with archive)
    std::vector<std::string> groups;    // kind scene / world: the stock asset groups (.assetgroup) that read the archive first
    json::Value extra;
};
SWOFTY_RECORD(BoxAssets, package, kind, entry, textures, root, srgb, archive, resources, groups)
SWOFTY_RECORD_REGISTRY(BoxAssets, "content.box_assets")

inline Registry<Package> packages{"content.package", {.applies = Applies::content}};
inline Registry<EraBox> era_boxes{"era.box", {.applies = Applies::read}};
inline Registry<PackageUi> package_ui{"ui.per_package", {.applies = Applies::content}};
inline Registry<BoxAssets> box_assets{"content.box_assets", {.applies = Applies::boot}};

}

namespace swofty::detail {

inline std::atomic<const SwoftyContentOps *> &content_ops_override()
{
    static std::atomic<const SwoftyContentOps *> t{nullptr};
    return t;
}

inline void set_content_ops_table(const SwoftyContentOps *t) { content_ops_override() = t; }

inline const SwoftyContentOps *content_ops_table()
{
    if (auto t = content_ops_override().load())
        return t->version >= 1 ? t : nullptr;
#if defined(_WIN32)
    static std::atomic<const SwoftyContentOps *> found{nullptr};
    if (auto t = found.load())
        return t;
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    auto get = core ? reinterpret_cast<SwoftyCoreContentOpsFn>(GetProcAddress(core, "SwoftyCoreContentOps")) : nullptr;
    const SwoftyContentOps *t = get ? get() : nullptr;
    if (t && t->version < 1)
        t = nullptr;
    found = t;
    return t;
#else
    return nullptr;
#endif
}

}

namespace swofty::content {

enum class Ownership { unknown, owned, not_owned };

// Whether the player owns the content behind an entitlement name (standard library 1.8, experimental). "f1_2026"
// is the F1 2026 season content. unknown until the game has the answer, for any other name, and on a core or
// build without the game binding. A cache read: safe from any thread.
inline Ownership owned(std::string_view entitlement)
{
    auto t = swofty::detail::content_ops_table();
    if (!t || !t->owned)
        return Ownership::unknown;
    int r = t->owned(std::string(entitlement).c_str());
    return r == SWOFTY_OWNERSHIP_OWNED ? Ownership::owned : r == SWOFTY_OWNERSHIP_NOT_OWNED ? Ownership::not_owned : Ownership::unknown;
}

}
