#pragma once
#include <string>

#include "core/boxes/car_category_logic.hpp"
#include "core/frontend/tile_records.hpp"

// Which template a ui.tile record's tile is built with and what its image leaf gets (home_tiles.cpp, career/content_tiles.cpp).
// A stock tile template has no image leaf; the launcher's UI build adds the image templates below only when an enabled
// ui.tile record names an image, and says so with the ui\tile_images.flag file. Pure logic: no game calls.

namespace frontend::tile_image {

constexpr const char *FLAG = "ui\\tile_images.flag";
constexpr const char *LEAF = "image";
constexpr const char *HOME_TEMPLATE = "addon_tile_image_default";
constexpr const char *ROW_TEMPLATE = "addon_tile_image_row";

enum class Step { none, template_missing, unresolved, set };

struct Plan {
    Step step = Step::none;
    std::string path, why;
    const char *template_name = nullptr;
    tile_records::Image image;
};

inline const char *template_of(bool in_row) { return in_row ? ROW_TEMPLATE : HOME_TEMPLATE; }

inline Plan plan(const tile_records::Tile &t, const tile_records::SliceOf &slice_of, bool template_built, bool in_row)
{
    Plan p;
    p.image = tile_records::resolve_image(t, slice_of);
    auto &image = p.image;
    switch (image.kind) {
    case tile_records::Image::Kind::none:
        return p;
    case tile_records::Image::Kind::unresolved:
        p.step = Step::unresolved;
        p.why = image.why;
        return p;
    case tile_records::Image::Kind::slice:
        break;
    }
    p.path = runtime::car_category::slice_path(image.array, image.name);
    if (!template_built) {
        p.step = Step::template_missing;
        return p;
    }
    p.step = Step::set;
    p.template_name = template_of(in_row);
    return p;
}

struct Counts {
    size_t set = 0, template_missing = 0, unresolved = 0;
};

inline std::string summary(const char *place, const Counts &c)
{
    std::string out;
    auto add = [&](size_t n, const char *what) {
        if (n)
            out += (out.empty() ? "" : ", ") + std::to_string(n) + " " + what;
    };
    add(c.set, "given their image");
    add(c.unresolved, "naming no slice");
    add(c.template_missing, "without the UI build's image template");
    return std::string("[ui.tile] ") + place + ": tile images: " + (out.empty() ? "none" : out);
}

}
