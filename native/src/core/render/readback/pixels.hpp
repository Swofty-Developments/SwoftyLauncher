#pragma once
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

namespace render::readback {

// Tight RGBA rows from mapped readback rows (pitch bytes apart), alpha kept, BGRA swapped to RGBA, scaled down
// nearest-neighbour to at most `side` on the longest edge with the aspect ratio kept. Nearest-neighbour keeps alpha
// edges free of colour fringes. Empty when the source is larger than max_source on a side or the pitch is short.
inline std::vector<uint8_t> fit(const uint8_t *source, unsigned width, unsigned height, unsigned pitch, bool bgra,
                                unsigned max_source, unsigned side, unsigned &out_w, unsigned &out_h)
{
    if (!source || !width || !height || !side || width > max_source || height > max_source || pitch < width * 4)
        return {};
    const unsigned longest = std::max(width, height);
    out_w = longest <= side ? width : std::max(1u, width * side / longest);
    out_h = longest <= side ? height : std::max(1u, height * side / longest);
    std::vector<uint8_t> out(size_t(out_w) * out_h * 4);
    const bool same = out_w == width && out_h == height;
    for (unsigned y = 0; y < out_h; y++) {
        const uint8_t *row = source + size_t(same ? y : size_t(y) * height / out_h) * pitch;
        uint8_t *to = out.data() + size_t(y) * out_w * 4;
        if (same)
            std::memcpy(to, row, size_t(width) * 4);
        else
            for (unsigned x = 0; x < out_w; x++)
                std::memcpy(to + size_t(x) * 4, row + size_t(x * width / out_w) * 4, 4);
        if (bgra)
            for (unsigned x = 0; x < out_w; x++)
                std::swap(to[x * 4], to[x * 4 + 2]);
    }
    return out;
}

}
