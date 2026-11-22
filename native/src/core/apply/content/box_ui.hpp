#pragma once

#include "core/apply/content/erp_file.hpp"

#include <swofty/json.hpp>

#include <algorithm>
#include <cstdint>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <vector>

// ui.per_package (experimental, SWOFTY_BOX_UI=1): one box's own UI textures. The core writes a copy of each archive
// a record lists with the record's textures swapped in, and while the box is the pressed one the file route sends the
// game's opens of that archive to the copy (scope SCOPE, put back when the box is left). Pure, for the host test: the
// glue (box_ui.cpp) reads the files and installs the routes.
//
// Which ui_package archives the game reloads, from the game's own FLOW log (swofty sessions 20261024-234510 and
// 20261031-125459, FlowCoordinator::processNextPreload): preload state 1 is set at StatePersistent and loads one flow
// that no later state unloads; state 2 (front end) loads the front-end flows; state 4 (session) unloads a front-end
// flow, loads the race flows and the hud screen elements at each session load from the front end, and unloads them
// again when the session goes back to the front end (CURRENT:4 -> NEW:2: eUnload FLOW 34, 12, 10, eUnloadElements
// hud, ...); a session after a session (CURRENT:4 -> NEW:4) reloads nothing. UIFontAssetsSubTask runs once in
// StateEgoBoot. The docs call persistent.erp and flow_persistent.erp the shared archives every screen may use. Which
// flow number is which archive is not read from the exe: RESIDENT is the archives named after that boot flow.
// When two loaded archives hold the same eaid name the resource map keeps the first resource created
// (re/db/subsystems/assets.md, L2), so a texture a resident archive also holds stays stock whatever the copies hold.

namespace swofty_core::apply::content::box_ui {

using Value = swofty::json::Value;

inline constexpr const char *REGISTRY = "ui.per_package";
inline constexpr const char *CACHE_ADDON = "swofty.box_ui";
inline constexpr const char *SCOPE = "content:ui.per_package";
inline constexpr const char *UI_FOLDER = "2025_asset_groups/ui_package/";
inline constexpr const char *DEFAULT_ROOT = "eaid://ui_package/art/";
inline constexpr const char *FENCE_LINE = "ui.per_package: off (experimental; set SWOFTY_BOX_UI=1 to swap box UI textures)";

struct Resident {
    const char *path;
    bool prefix;
};
inline constexpr Resident RESIDENT[] = {
    {"2025_asset_groups/ui_package/persistent.erp", false},
    {"2025_asset_groups/ui_package/flow_persistent.erp", false},
    {"2025_asset_groups/ui_package/screens/persistent.erp", false},
    {"2025_asset_groups/ui_package/fonts", true},
};

// Whether the game keeps this archive loaded from boot.
inline bool resident(std::string_view game_path)
{
    for (const auto &r : RESIDENT)
        if (r.prefix ? game_path.rfind(r.path, 0) == 0 : game_path == r.path)
            return true;
    return false;
}

inline std::string lowered(std::string s)
{
    for (char &c : s) {
        if (c == '\\')
            c = '/';
        else if (c >= 'A' && c <= 'Z')
            c = char(c + 32);
    }
    return s;
}

// A record as the glue uses it. textures is a folder (png / dds) or an archive with a resource list.
struct Record {
    std::string id, addon, package, folder, erp, root = DEFAULT_ROOT;
    std::vector<std::string> resources, archives;
    bool srgb = true;
    std::vector<std::string> lines;
    bool ok() const { return lines.empty(); }
};

inline std::string addon_of(std::string_view id)
{
    auto slash = id.rfind('/');
    return slash == std::string_view::npos ? std::string() : std::string(id.substr(0, slash));
}

inline Record parse(const std::string &id, const Value &rec)
{
    Record r;
    r.id = id;
    r.addon = addon_of(id);
    const std::string who = std::string(REGISTRY) + " " + id + ": ";
    if (rec["package"].is_string())
        r.package = std::string(rec["package"].as_string());
    else
        r.lines.push_back(who + "names no package");
    if (rec["root"].is_string())
        r.root = std::string(rec["root"].as_string());
    if (rec["srgb"].is_bool())
        r.srgb = rec["srgb"].as_bool();
    const Value &t = rec["textures"];
    if (t.is_string())
        r.folder = std::string(t.as_string());
    else if (t.is_object() && t["erp"].is_string() && t["resources"].is_array()) {
        r.erp = std::string(t["erp"].as_string());
        for (std::size_t i = 0; i < t["resources"].size(); i++)
            if (t["resources"][i].is_string())
                r.resources.push_back(std::string(t["resources"][i].as_string()));
    } else
        r.lines.push_back(who + "textures is neither a folder nor { erp, resources }");
    const Value &a = rec["archives"];
    for (std::size_t i = 0; a.is_array() && i < a.size(); i++) {
        if (!a[i].is_string())
            continue;
        std::string p = lowered(std::string(a[i].as_string()));
        if (p.rfind(UI_FOLDER, 0) != 0 || p.size() < 5 || p.compare(p.size() - 4, 4, ".erp") != 0 ||
            p.find("..") != std::string::npos)
            r.lines.push_back(who + p + " is not a ui_package archive");
        else if (std::find(r.archives.begin(), r.archives.end(), p) == r.archives.end())
            r.archives.push_back(p);
    }
    if (r.archives.empty())
        r.lines.push_back(who + "lists no archive");
    return r;
}

// The records of the pressed box: those of the first package along its variant chain that has any.
inline std::vector<Record> for_box(const std::vector<std::string> &chain, const std::vector<Record> &all)
{
    for (const auto &package : chain) {
        std::vector<Record> out;
        for (const auto &r : all)
            if (r.package == package)
                out.push_back(r);
        if (!out.empty())
            return out;
    }
    return {};
}

// The surface and its view share one stem: <stem>.tif.image (GfxSurfaceRes) and <stem>.tif (GfxSRVResource, which
// names the surface's format and levels). They are swapped together or not at all.
inline std::string stem_of(const std::string &name)
{
    constexpr std::string_view IMAGE = ".image";
    if (name.size() > IMAGE.size() && name.compare(name.size() - IMAGE.size(), IMAGE.size(), IMAGE) == 0)
        return name.substr(0, name.size() - IMAGE.size());
    return name;
}

inline std::string type_of(const erpfile::Entry &e)
{
    std::string t = e.mid.substr(0, 16);
    auto nul = t.find('\0');
    return nul == std::string::npos ? t : t.substr(0, nul);
}

// The swaps for one archive: every resource of the payload (or of `only`, when it lists names) that the archive also
// holds, with the same resource type and fragment count, as whole fragment lists, grouped by stem.
struct Plan {
    erpfile::Changes changes;
    std::vector<std::string> swapped;
    std::vector<std::string> lines;
};

inline bool plan(const erpfile::Archive &stock, const erpfile::Source &pay_src, const erpfile::Archive &pay,
                 const std::set<std::string> &only, Plan &out, std::string &why)
{
    std::map<std::string, std::vector<const erpfile::Entry *>> groups;
    for (const auto &e : pay.entries)
        if ((only.empty() || only.count(e.name)) && stock.find(e.name))
            groups[stem_of(e.name)].push_back(&e);
    for (auto &[stem, members] : groups) {
        std::string refusal;
        for (const char *suffix : {"", ".image"}) {
            const erpfile::Entry *s = stock.find(stem + suffix);
            if (!s)
                continue;
            const erpfile::Entry *p = nullptr;
            for (auto *m : members)
                if (m->name == s->name)
                    p = m;
            if (!p)
                refusal = s->name + " has no copy in the textures (a surface and its view go together)";
            else if (type_of(*p) != type_of(*s))
                refusal = s->name + " is a " + type_of(*s) + " in the archive and a " + type_of(*p) + " in the textures";
            else if (p->frags.size() != s->frags.size())
                refusal = s->name + " has " + std::to_string(s->frags.size()) + " fragment(s) in the archive and " +
                          std::to_string(p->frags.size()) + " in the textures";
            if (!refusal.empty())
                break;
        }
        if (!refusal.empty()) {
            out.lines.push_back(stem + " kept: " + refusal);
            continue;
        }
        for (auto *m : members) {
            std::vector<std::optional<std::string>> frags;
            for (const auto &f : m->frags) {
                std::string data;
                if (!erpfile::fragment(pay_src, pay, f, data, why))
                    return false;
                frags.push_back(std::move(data));
            }
            out.changes[m->name] = std::move(frags);
            out.swapped.push_back(m->name);
        }
    }
    return true;
}

// The cache file of one archive's copy: a hash of what decides its bytes.
inline std::string copy_name(const std::string &game_path, uint64_t key)
{
    std::string flat;
    for (char c : game_path.substr(std::string(UI_FOLDER).size()))
        flat += (c == '/' || c == '\\') ? '_' : c;
    static const char *HEX = "0123456789abcdef";
    std::string hex;
    for (int i = 60; i >= 0; i -= 4)
        hex += HEX[(key >> i) & 0xf];
    return "boxui_" + hex + "_" + flat;
}

inline uint64_t mix(uint64_t key, std::string_view t)
{
    for (unsigned char c : t) {
        key ^= c;
        key *= 0x100000001b3ull;
    }
    key ^= 0xff;
    return key * 0x100000001b3ull;
}

}
