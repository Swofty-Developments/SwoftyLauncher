#pragma once

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <string>
#include <string_view>
#include <vector>

#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-function"
#pragma GCC diagnostic ignored "-Wunused-parameter"
#pragma GCC diagnostic ignored "-Wsign-compare"
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#pragma GCC diagnostic ignored "-Wimplicit-fallthrough"
#pragma GCC diagnostic ignored "-Wcast-function-type"
#pragma GCC diagnostic ignored "-Wstringop-overflow"
#pragma GCC diagnostic ignored "-Wmaybe-uninitialized"
#endif
#define NANOSVG_IMPLEMENTATION
#define NANOSVGRAST_IMPLEMENTATION
#include "../../../../third_party/nanosvg/nanosvg.h"
#include "../../../../third_party/nanosvg/nanosvgrast.h"
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

namespace swofty_core::apply::content::svg {

inline constexpr uint32_t LONG_SIDE = 1024;
inline constexpr size_t MAX_BYTES = size_t(8) << 20;

inline bool is_svg(std::string_view file, std::string_view bytes)
{
    auto ends = [&](std::string_view s) {
        return file.size() >= s.size() && std::equal(s.rbegin(), s.rend(), file.rbegin(), [](char a, char b) {
                   return a == (b >= 'A' && b <= 'Z' ? char(b + 32) : b);
               });
    };
    if (ends(".svg"))
        return true;
    return bytes.substr(0, 512).find("<svg") != std::string_view::npos;
}

inline bool decode(std::string_view data, std::string &rgba, uint32_t &w, uint32_t &h, std::string &why)
{
    if (data.empty() || data.size() > MAX_BYTES)
        return why = data.empty() ? "the SVG is empty" : "the SVG is larger than 8 MB", false;
    std::vector<char> text(data.begin(), data.end());
    text.push_back('\0');
    NSVGimage *image = nsvgParse(text.data(), "px", 96.0f);
    if (!image)
        return why = "not an SVG the parser reads", false;
    float iw = image->width, ih = image->height;
    if (!(iw >= 1.0f) || !(ih >= 1.0f) || iw > 65536.0f || ih > 65536.0f) {
        nsvgDelete(image);
        return why = "the SVG has no usable width and height", false;
    }
    float scale = float(LONG_SIDE) / std::max(iw, ih);
    w = std::max<uint32_t>(1, uint32_t(iw * scale + 0.5f));
    h = std::max<uint32_t>(1, uint32_t(ih * scale + 0.5f));
    NSVGrasterizer *rast = nsvgCreateRasterizer();
    if (!rast) {
        nsvgDelete(image);
        return why = "the SVG rasteriser did not start", false;
    }
    rgba.assign(size_t(w) * h * 4, '\0');
    nsvgRasterize(rast, image, 0, 0, scale, reinterpret_cast<unsigned char *>(rgba.data()), int(w), int(h), int(w) * 4);
    nsvgDeleteRasterizer(rast);
    nsvgDelete(image);
    for (size_t i = 0; i < rgba.size(); i += 4)
        if (!rgba[i + 3])
            rgba[i] = rgba[i + 1] = rgba[i + 2] = 0;
    return true;
}

}
