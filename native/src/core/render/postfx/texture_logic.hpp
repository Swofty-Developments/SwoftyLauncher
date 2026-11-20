#pragma once
#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <string_view>
#include <vector>

namespace render::postfx {

// DXGI_FORMAT values for the texture format names.
inline int format_of(std::string_view name)
{
    struct Named {
        std::string_view name;
        int dxgi;
    };
    static constexpr Named NAMED[] = {{"rgba8", 28}, {"rgba16f", 10}, {"rg16f", 34},     {"r16f", 54},
                                      {"r32f", 41},  {"rgb10a2", 24}, {"r11g11b10f", 26}};
    for (auto &n : NAMED)
        if (n.name == name)
            return n.dxgi;
    return 0;
}

struct Size {
    uint32_t w = 0, h = 0;
};

inline constexpr uint32_t MAX_MIPS = 12;

// The levels a w x h texture carries for a declared count: at least 1, at most MAX_MIPS and
// floor(log2(max(w, h))) + 1.
inline uint32_t mip_levels(uint32_t w, uint32_t h, uint32_t declared)
{
    uint32_t most = 1;
    for (uint32_t m = w > h ? w : h; m > 1; m >>= 1)
        most++;
    uint32_t cap = most < MAX_MIPS ? most : MAX_MIPS;
    return declared < 1 ? 1 : declared > cap ? cap : declared;
}

// The size of a level: each step halves, rounding down, at least 1.
inline Size mip_size(uint32_t w, uint32_t h, uint32_t level)
{
    uint32_t sw = level < 32 ? w >> level : 0, sh = level < 32 ? h >> level : 0;
    return {sw ? sw : 1u, sh ? sh : 1u};
}

// A texture's size for a picture: its own width and height, or the picture's scaled, at least 1x1.
inline Size texture_size(double scale, uint32_t width, uint32_t height, uint32_t pic_w, uint32_t pic_h)
{
    if (width && height)
        return {width, height};
    auto at = [&](uint32_t v) {
        double s = v * scale + 0.5;
        return s < 1 ? 1u : uint32_t(s);
    };
    return {at(pic_w), at(pic_h)};
}

inline uint32_t groups(uint32_t size, uint32_t threads) { return threads ? (size + threads - 1) / threads : 0; }

// Scales an RGB24 picture to dw x dh by area-weighted averaging: every output pixel is the mean of the
// source area it covers, partial pixels by their share.
inline void box_scale_rgb(const uint8_t *src, uint32_t sw, uint32_t sh, uint8_t *dst, uint32_t dw, uint32_t dh)
{
    if (!sw || !sh || !dw || !dh)
        return;
    std::vector<float> row(size_t(sw) * 3);
    double fx = double(sw) / dw, fy = double(sh) / dh;
    for (uint32_t dy = 0; dy < dh; dy++) {
        std::fill(row.begin(), row.end(), 0.0f);
        double y0 = dy * fy, y1 = y0 + fy;
        for (uint32_t sy = uint32_t(y0); sy < sh && sy < y1; sy++) {
            double w = std::min<double>(sy + 1, y1) - std::max<double>(sy, y0);
            const uint8_t *line = src + size_t(sy) * sw * 3;
            for (size_t i = 0; i < size_t(sw) * 3; i++)
                row[i] += float(w) * line[i];
        }
        for (uint32_t dx = 0; dx < dw; dx++) {
            double x0 = dx * fx, x1 = x0 + fx, acc[3] = {};
            for (uint32_t sx = uint32_t(x0); sx < sw && sx < x1; sx++) {
                double w = std::min<double>(sx + 1, x1) - std::max<double>(sx, x0);
                for (int c = 0; c < 3; c++)
                    acc[c] += w * row[size_t(sx) * 3 + c];
            }
            for (int c = 0; c < 3; c++) {
                double v = acc[c] / (fx * fy) + 0.5;
                dst[(size_t(dy) * dw + dx) * 3 + c] = uint8_t(v > 255 ? 255 : v);
            }
        }
    }
}

}
