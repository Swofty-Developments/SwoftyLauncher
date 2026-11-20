#pragma once
#include "core/hud/drawlist/frame.hpp"

#include <cmath>
#include <cstdint>

namespace hud::drawlist {

// ----- v1 calls ---------------------------------------------------------------------------------

inline void rect(Frame &f, float x, float y, float w, float h, uint32_t rgba)
{
    if (w <= 0 || h <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    Pt p[4];
    rect_points(f.xform, x, y, w, h, p);
    quad_flat(f, p, pack(rgba, f.opacity));
}

inline void rect_outline(Frame &f, float x, float y, float w, float h, float thickness, uint32_t rgba)
{
    if (w <= 0 || h <= 0 || thickness <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    float t = thickness;
    rect(f, x, y, w, t, rgba);
    rect(f, x, y + h - t, w, t, rgba);
    rect(f, x, y + t, t, h - 2 * t, rgba);
    rect(f, x + w - t, y + t, t, h - 2 * t, rgba);
}

inline void line(Frame &f, float x0, float y0, float x1, float y1, float thickness, uint32_t rgba)
{
    if (thickness <= 0 || !rgba_visible(rgba) || (x0 == x1 && y0 == y1) || clipped_out(f))
        return;
    float dx = x1 - x0, dy = y1 - y0, len = std::sqrt(dx * dx + dy * dy);
    float nx = -dy / len * thickness * 0.5f, ny = dx / len * thickness * 0.5f;
    Pt p[4];
    f.xform.apply(x0 + nx, y0 + ny, p[0].x, p[0].y);
    f.xform.apply(x1 + nx, y1 + ny, p[1].x, p[1].y);
    f.xform.apply(x1 - nx, y1 - ny, p[2].x, p[2].y);
    f.xform.apply(x0 - nx, y0 - ny, p[3].x, p[3].y);
    quad_flat(f, p, pack(rgba, f.opacity));
}

inline void image_uv(Frame &f, int slot, float x, float y, float w, float h, float u0, float v0, float u1, float v1,
                     uint32_t tint)
{
    if (slot < 0 || w <= 0 || h <= 0 || !rgba_visible(tint) || clipped_out(f))
        return;
    if (!reserve(f, 6, slot + 1))
        return;
    Pt p[4];
    rect_points(f.xform, x, y, w, h, p);
    uint32_t col = pack(tint, f.opacity);
    float u[4] = {u0, u1, u1, u0}, v[4] = {v0, v0, v1, v1};
    for (int k : {0, 1, 3, 1, 2, 3})
        vtx(f, p[k].x, p[k].y, col, u[k], v[k], IMAGE);
}

inline void image(Frame &f, int slot, float x, float y, float w, float h, uint32_t tint)
{
    image_uv(f, slot, x, y, w, h, 0, 0, 1, 1, tint);
}

}
