#pragma once
#include <cstddef>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <vector>

#include <swofty/json.hpp>

#include "core/apply/content/slices.hpp"
#include "core/assets/lang_scope.hpp"
#include "core/hooks/context.hpp"

// The ui.tile records a screen builds its extra tiles from. Pure logic, shared by every place (home, f1_world, career,
// car_category): parse a record, drop the ones of another place, dedupe by id, test the gate against the current facts, cap
// the count. The gate is a selector over mode, career and content, compiled by lang_scope::compile_within (the lang.string
// within), so there is one selector language. Records come in load order (registry::older::in_load_order).

namespace frontend::tile_records {

namespace lang_scope = swofty_core::lang_scope;
namespace hooks = swofty_core::hooks;
using swofty::json::Value;

inline constexpr const char *REGISTRY = "ui.tile";
inline constexpr size_t TILE_CAP = 255;

enum class Place { home, career, f1_world, car_category };

inline const char *place_name(Place p)
{
    switch (p) {
    case Place::home:
        return "home";
    case Place::career:
        return "career";
    case Place::f1_world:
        return "f1_world";
    case Place::car_category:
        return "car_category";
    }
    return "";
}

inline std::optional<Place> place_of(std::string_view name)
{
    for (Place p : {Place::home, Place::career, Place::f1_world, Place::car_category})
        if (name == place_name(p))
            return p;
    return std::nullopt;
}

struct Tile {
    std::string id;
    Place place = Place::home;
    std::string title;
    bool literal = false;
    std::string image, route;
    bool route_told = false;
    std::shared_ptr<const hooks::Selectors> gate;
};

struct Input {
    std::string id;
    const Value *record = nullptr;
};

using RouteExists = std::function<bool(const std::string &)>;

struct Options {
    bool need_route = false;
    size_t cap = TILE_CAP;
    RouteExists route_exists;
};

// The route name the game gets for a tile's `route`. A route that names an enabled ui.route record by its id
// (`<addon>/<key>`) is the key: the build clones the route group under that name. Any other route (a stock route name,
// or a bare key) passes through unchanged. `unknown` is set when the route looks like a record id but no enabled record
// has it, so the caller can say so; the string is then passed as written.
inline std::string game_route(const std::string &route, const RouteExists &exists, bool *unknown = nullptr)
{
    if (unknown)
        *unknown = false;
    auto at = route.find('/');
    if (at == std::string::npos || !exists)
        return route;
    if (exists(route))
        return route.substr(at + 1);
    if (unknown)
        *unknown = true;
    return route;
}

inline std::string unknown_route_line(const std::string &tile_id, const std::string &route)
{
    return std::string(REGISTRY) + " " + tile_id + ": route " + route +
           " names no enabled ui.route record; the game gets it as written";
}

struct Result {
    std::vector<Tile> tiles;
    std::vector<std::string> lines;
    size_t other_place = 0, gated_out = 0, refused = 0, deduped = 0, capped = 0;
};

inline std::string text_of(const Value &v) { return v.is_string() ? std::string(v.as_string()) : std::string(); }

inline bool parse(const std::string &id, const Value &rec, Tile &out, std::string &why)
{
    out = Tile{};
    out.id = id;
    auto place = place_of(text_of(rec["place"]));
    if (!place) {
        why = "place \"" + text_of(rec["place"]) + "\" is not home, career, f1_world or car_category";
        return false;
    }
    out.place = *place;
    const Value &title = rec["title"];
    if (title.is_string()) {
        out.title = std::string(title.as_string());
    } else if (title.is_object() && title["literal"].is_string()) {
        out.title = std::string(title["literal"].as_string());
        out.literal = true;
    }
    if (out.title.empty()) {
        why = "title is missing: a lang key or { \"literal\" }";
        return false;
    }
    out.image = text_of(rec["image"]);
    out.route = text_of(rec["route"]);
    if (!lang_scope::compile_within(rec["gate"], out.gate, &why)) {
        why = "gate does not compile: " + why;
        return false;
    }
    return true;
}

inline bool gate_passes(const Tile &t, const hooks::Facts *facts) { return !t.gate || (facts && t.gate->match(*facts)); }

// Records of `place` that apply under `facts` (null: no facts yet, a gated tile does not show), in load order, one per id
// (a repeated id keeps the first position and the later record's content), at most opt.cap of them.
inline Result effective(const std::vector<Input> &in, Place place, const hooks::Facts *facts, const Options &opt = {})
{
    Result out;
    std::vector<Tile> parsed;
    std::map<std::string, size_t> at;
    for (auto &i : in) {
        if (!i.record)
            continue;
        Tile t;
        std::string why;
        if (!parse(i.id, *i.record, t, why)) {
            out.refused++;
            out.lines.push_back(std::string(REGISTRY) + " " + i.id + ": not used: " + why);
            continue;
        }
        if (t.place != place) {
            out.other_place++;
            continue;
        }
        if (!t.route.empty() && opt.route_exists) {
            bool unknown = false;
            t.route = game_route(t.route, opt.route_exists, &unknown);
            if (unknown) {
                t.route_told = true;
                out.lines.push_back(unknown_route_line(t.id, t.route));
            }
        }
        if (opt.need_route && t.route.empty()) {
            out.refused++;
            out.lines.push_back(std::string(REGISTRY) + " " + i.id + ": not used: a " + place_name(place) +
                                " tile needs a route to open");
            continue;
        }
        auto was = at.find(t.id);
        if (was != at.end()) {
            out.deduped++;
            out.lines.push_back(std::string(REGISTRY) + " " + t.id + ": listed twice; the later record is used");
            parsed[was->second] = std::move(t);
            continue;
        }
        at[t.id] = parsed.size();
        parsed.push_back(std::move(t));
    }
    for (auto &t : parsed) {
        if (!gate_passes(t, facts)) {
            out.gated_out++;
            continue;
        }
        if (out.tiles.size() >= opt.cap) {
            out.capped++;
            continue;
        }
        out.tiles.push_back(std::move(t));
    }
    if (out.capped)
        out.lines.push_back(std::string(REGISTRY) + " " + place_name(place) + ": " + std::to_string(out.capped) +
                            " tile(s) over the cap of " + std::to_string(opt.cap) + " are not shown");
    return out;
}

// How a tile's title goes into the VM: a lang key is localised (the lang table, lang.string overrides included), a
// literal is not.
struct TitleText {
    const char *text;
    bool localised;
};

inline TitleText title_text(const Tile &t) { return {t.title.c_str(), !t.literal}; }

struct Image {
    enum class Kind { none, slice, unresolved } kind = Kind::none;
    std::string array, name, why;
    bool literal = false;
};

// `slice_of` maps a ui.texture_slice record id to its {array, name}. An image is a slice record id or a `#array[name]` string.
using SliceOf = std::function<std::optional<swofty_core::apply::content::slices::Ref>(const std::string &)>;

inline Image resolve_image(const Tile &t, const SliceOf &slice_of)
{
    namespace slices = swofty_core::apply::content::slices;
    Image img;
    if (t.image.empty())
        return img;
    std::optional<slices::Ref> ref = t.image.front() == '#' ? slices::parse_ref(t.image)
                                     : slice_of             ? slice_of(t.image)
                                                            : std::nullopt;
    if (!ref) {
        img.kind = Image::Kind::unresolved;
        img.why = t.image.find('/') != std::string::npos
                      ? "image slice " + t.image + " is not provided by any enabled ui.texture_slice record"
                      : "image " + t.image + " is neither a ui.texture_slice record id nor a #array[name] reference";
        return img;
    }
    img.kind = Image::Kind::slice;
    img.array = ref->array;
    img.name = ref->name;
    img.literal = t.image.front() == '#';
    return img;
}

// The warning for a `#array[name]` image that no enabled ui.texture_slice record provides (`provided` holds
// "<array>|<name>"), once per slice (`told` remembers them); empty for any other image, a slice record, or a repeat. A
// slice of a stock array the game ships still shows, so it is only a warning.
inline std::string missing_slice_warning(const Tile &t, const Image &img, const std::set<std::string> &provided,
                                         std::set<std::string> &told)
{
    if (img.kind != Image::Kind::slice || !img.literal)
        return {};
    std::string key = img.array + "|" + img.name;
    if (provided.count(key) || !told.insert(key).second)
        return {};
    return t.id + ": image slice " + img.name + " of " + img.array +
           " is not provided by any enabled ui.texture_slice record (a slice the game ships in that array still shows)";
}

}
