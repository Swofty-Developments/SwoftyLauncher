#pragma once
#include "core/render/readback/pixels.hpp"

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace badge_images::data {
// Metadata tracks at most 1024 recognised names; at most 32 GPU copies can be
// in flight. Completed runtime pixels/resources have their own 32-entry bound.
inline constexpr unsigned SIDE = 64, PIXELS = SIDE * SIDE * 4, CACHE = 1024, PENDING = 32;

// D3D debug names may include the engine's resource URI or a .image suffix.
// Match the complete Live API identifier, not a table/index substring.
inline std::optional<std::string> reference(std::string_view name)
{
    constexpr std::string_view prefix = "render_badge_";
    auto at = name.find(prefix);
    if (at == std::string_view::npos)
        return {};
    if (at && name[at - 1] != '/')
        return {};
    auto suffix = name.substr(at + prefix.size());
    if (suffix.ends_with(".image"))
        suffix.remove_suffix(6);
    if (suffix == "default.tga")
        return "eaid://ui_package/art/textures/render_badge_default.tga";
    auto space = suffix.find(' ');
    if (space == std::string_view::npos)
        return {};
    auto table = suffix.substr(0, space), number = suffix.substr(space + 1);
    if ((table != "ePreLoaded" && table != "eUserDefined" && table != "eNetworkReplicated") || number.empty() ||
        number.size() > 5)
        return {};
    unsigned index = 0;
    for (unsigned char c : number) {
        if (c < '0' || c > '9')
            return {};
        index = index * 10 + c - '0';
    }
    if (index >= 0x3fff)
        return {};
    return "eaid://ui_package/art/textures/render_badge_" + std::string(table) + " " + std::to_string(index);
}

// Tight RGBA rows at most SIDE on a side (render/readback/pixels.hpp).
inline std::vector<uint8_t> pixels(const uint8_t *source, unsigned width, unsigned height, unsigned pitch, bool bgra,
                                   unsigned &out_w, unsigned &out_h)
{
    return render::readback::fit(source, width, height, pitch, bgra, 512, SIDE, out_w, out_h);
}
}
