#pragma once

#include "core/apply/content/lower.hpp"

#include <swofty/json.hpp>

#include <algorithm>
#include <cctype>
#include <functional>
#include <optional>
#include <map>
#include <set>
#include <string>
#include <string_view>
#include <vector>

namespace swofty_core::apply::content::slices {

using swofty::json::Value;

inline constexpr const char *REGISTRY = "ui.texture_slice";
inline constexpr const char *CONTEXT = "ui.texture_slice";

struct Known {
    const char *array;
    const char *alias;
};

// The texture arrays the core grows: the team logo arrays (teams.hpp logo_arrays) and the home screen's 6x6 tile
// array. The first column is the name the game's overrides.uidb lists (overrides.<array>); the second is a spelling the
// docs used before that is still read.
inline constexpr Known KNOWN[] = {
    {"teams.driver_select_team_logos", "teams.driver_select_team_logo"},
    {"team_logos.driver_career_2024_team_select", ""},
    {"teams.mp_team_select_logos", ""},
    {"osd.timing_tower.team_badges_hud", ""},
    {"osd.timing_tower.team_badges_broadcast", ""},
    {"teams.badges_broadcast", ""},
    {"broadcast_ui_generic.grid_team_logos", ""},
    {"home_screen.tiles_solo_6x6", ""},
};

inline std::string known_list()
{
    std::string out;
    for (auto &k : KNOWN)
        out += (out.empty() ? "" : ", ") + std::string("overrides.") + k.array;
    return out;
}

// The array a record names, in the form deliver_logos wants (no "overrides." prefix, the game's own spelling), or "".
inline std::string canonical_array(std::string_view name)
{
    constexpr std::string_view PREFIX = "overrides.";
    if (name.substr(0, PREFIX.size()) == PREFIX)
        name.remove_prefix(PREFIX.size());
    for (auto &k : KNOWN)
        if (name == k.array || (*k.alias && name == k.alias))
            return k.array;
    return {};
}

inline std::string owner_of(std::string_view id)
{
    auto slash = id.rfind('/');
    return slash == std::string_view::npos ? std::string() : std::string(id.substr(0, slash));
}

inline bool image_extension(std::string_view file)
{
    auto dot = file.rfind('.');
    if (dot == std::string_view::npos)
        return false;
    std::string ext(file.substr(dot + 1));
    for (auto &c : ext)
        c = char(std::tolower(static_cast<unsigned char>(c)));
    return ext == "png" || ext == "svg";
}

struct ArrayCount {
    std::size_t slices = 0;
    std::vector<std::string> addons;
};

struct Lowering {
    std::vector<LogoImage> images;
    std::vector<std::string> lines;
    std::map<std::string, ArrayCount> arrays;
    std::set<std::string> provided;
    int applied = 0, refused = 0, contested = 0;
};

struct Input {
    std::string id;
    const Value *record = nullptr;
};

// Each record, in load order, becomes one LogoImage: the slice `name` of `array`, the image a file of the addon.
// Two records that name the same slice are contested and the later one wins; a record whose array is not one the
// core grows, whose name is not usable or whose image is not a PNG or SVG file of the addon is refused and says why.
// `exists` answers whether an addon file is there (addon, relative path).
inline Lowering lower(const std::vector<Input> &in, const std::function<bool(const std::string &, const std::string &)> &exists)
{
    Lowering out;
    std::map<std::string, std::size_t> at;
    std::map<std::string, std::string> owner;
    for (auto &i : in) {
        if (!i.record)
            continue;
        const Value &rec = *i.record;
        std::string addon = owner_of(i.id);
        auto refuse = [&](const std::string &why) {
            out.refused++;
            out.lines.push_back(std::string(REGISTRY) + " " + i.id + ": not applied: " + why);
        };
        std::string given = rec["array"].is_string() ? std::string(rec["array"].as_string()) : std::string();
        std::string array = canonical_array(given);
        if (array.empty()) {
            refuse("array " + (given.empty() ? std::string("is missing") : given + " is not a texture array the core grows") +
                   " (known: " + known_list() + ")");
            continue;
        }
        std::string name = rec["name"].is_string() ? std::string(rec["name"].as_string()) : std::string();
        if (name.empty() || name.find_first_of(" \t#[]") != std::string::npos) {
            refuse("name \"" + name + "\" is empty or holds a space, # or bracket");
            continue;
        }
        if (!rec["image"].is_string() || std::string(rec["image"].as_string()).empty()) {
            refuse("image must be a PNG or SVG file in the addon's folder");
            continue;
        }
        std::string file = std::string(rec["image"].as_string());
        if (!image_extension(file)) {
            refuse("image " + file + " is not a .png or .svg file");
            continue;
        }
        if (exists && !exists(addon, file)) {
            refuse("image " + file + " is not in swofty/addons/" + addon + "/");
            continue;
        }
        LogoImage img;
        img.array = array;
        img.key = name;
        img.file = file;
        img.context = CONTEXT;
        img.addon = addon;
        img.id = i.id;
        std::string key = array + "|" + name;
        auto was = at.find(key);
        if (was == at.end()) {
            at[key] = out.images.size();
            out.images.push_back(std::move(img));
            out.arrays[array].slices++;
        } else {
            out.contested++;
            out.lines.push_back(std::string(REGISTRY) + " " + array + "/" + name + ": contested between " + owner[key] + " and " +
                                addon + "; " + addon + " is later in the load order and wins");
            out.images[was->second] = std::move(img);
        }
        owner[key] = addon;
        out.provided.insert(key);
        out.applied++;
        auto &addons = out.arrays[array].addons;
        if (std::find(addons.begin(), addons.end(), addon) == addons.end())
            addons.push_back(addon);
    }
    for (auto &[array, c] : out.arrays) {
        std::string list;
        for (auto &a : c.addons)
            list += (list.empty() ? "" : ", ") + a;
        std::size_t clashes = 0;
        for (auto &l : out.lines)
            clashes += l.rfind(std::string(REGISTRY) + " " + array + "/", 0) == 0;
        out.lines.push_back(std::string(REGISTRY) + " " + array + ": " + std::to_string(c.slices) + " slice" +
                            (c.slices == 1 ? "" : "s") + " from " + std::to_string(c.addons.size()) + " addon" +
                            (c.addons.size() == 1 ? "" : "s") + " (" + list + "), " + std::to_string(clashes) + " contested");
    }
    return out;
}

// A `#<array>[<name>]` image string (an era box's or a tile's image) names one slice of one texture array.
struct Ref {
    std::string array, name;
};

inline std::optional<Ref> parse_ref(std::string_view image)
{
    constexpr std::string_view PREFIX = "overrides.";
    if (image.size() < 4 || image.front() != '#' || image.back() != ']')
        return std::nullopt;
    auto open = image.find('[');
    if (open == std::string_view::npos || open < 2)
        return std::nullopt;
    std::string_view array = image.substr(1, open - 1), name = image.substr(open + 1, image.size() - open - 2);
    if (name.empty())
        return std::nullopt;
    std::string canonical = canonical_array(array);
    if (canonical.empty()) {
        if (array.substr(0, PREFIX.size()) == PREFIX)
            array.remove_prefix(PREFIX.size());
        canonical = std::string(array);
    }
    return Ref{canonical, std::string(name)};
}

// Whether the slice an image string names is one an enabled ui.texture_slice record adds (`provided` holds
// "<array>|<name>", Lowering::provided). The empty string when it is, or when the string names no slice, or when it is
// the entrant list's own name or one of `stock_names` (the entrant lists of the base packages: a stock slice the game ships); else the warning to log. Only a warning: a slice of a
// stock array the game ships is not known here.
inline std::string missing_slice(std::string_view image, const std::set<std::string> &provided,
                                 std::string_view entrant_list = {}, const std::set<std::string> &stock_names = {})
{
    auto ref = parse_ref(image);
    if (!ref || provided.count(ref->array + "|" + ref->name) || ref->name == entrant_list || stock_names.count(ref->name))
        return {};
    return "image slice " + ref->name + " of " + ref->array +
           " is not provided by any enabled ui.texture_slice record (a slice "
           "the game ships in that array still shows)";
}

}
