#pragma once
#include "core/hud/drawlist/primitives.hpp"
#include "core/hud/drawlist/tessellate.hpp"
#include "swofty_api.h"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <vector>

namespace hud::drawlist {

// ----- v2 shapes --------------------------------------------------------------------------------

inline void rect_rounded(Frame &f, float x, float y, float w, float h, float radius, uint32_t rgba)
{
    if (w <= 0 || h <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    rounded_local(f.local, x, y, w, h, radius, f.xform.scale());
    to_screen(f.xform, f.local, f.path);
    fill_convex(f, f.path, pack(rgba, f.opacity));
}

inline void rect_rounded_outline(Frame &f, float x, float y, float w, float h, float radius, float thickness, uint32_t rgba)
{
    if (w <= 0 || h <= 0 || thickness <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    float t = std::min(thickness, std::min(w, h) * 0.5f), half = t * 0.5f;
    rounded_local(f.local, x + half, y + half, w - t, h - t, radius - half, f.xform.scale());
    to_screen(f.xform, f.local, f.path);
    stroke(f, f.path, true, t * f.xform.scale(), pack(rgba, f.opacity));
}

inline uint32_t lerp_rgba(uint32_t a, uint32_t b, float t)
{
    t = std::clamp(t, 0.0f, 1.0f);
    uint32_t out = 0;
    for (int s = 0; s < 32; s += 8) {
        float ca = float((a >> s) & 0xff), cb = float((b >> s) & 0xff);
        out |= uint32_t(std::lround(ca + (cb - ca) * t)) << s;
    }
    return out;
}

inline void rect_gradient(Frame &f, float x, float y, float w, float h, float radius, uint32_t from, uint32_t to, uint32_t flags)
{
    if (w <= 0 || h <= 0 || (!rgba_visible(from) && !rgba_visible(to)) || clipped_out(f))
        return;
    rounded_local(f.local, x, y, w, h, radius, f.xform.scale());
    to_screen(f.xform, f.local, f.path);
    bool horizontal = flags & SWOFTY_GRADIENT_HORIZONTAL;
    f.cols.resize(f.local.size());
    for (size_t i = 0; i < f.local.size(); i++)
        f.cols[i] = pack(lerp_rgba(from, to, horizontal ? (f.local[i].x - x) / w : (f.local[i].y - y) / h), f.opacity);
    fill_convex(f, f.path, f.cols);
}

inline void circle(Frame &f, float cx, float cy, float radius, uint32_t rgba)
{
    if (radius <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    int segs = std::max(8, arc_segments(radius * f.xform.scale(), 2 * PI));
    f.local.clear();
    arc_local(f.local, cx, cy, radius, 0, 2 * PI, segs);
    f.local.pop_back();
    to_screen(f.xform, f.local, f.path);
    fill_convex(f, f.path, pack(rgba, f.opacity));
}

inline void circle_outline(Frame &f, float cx, float cy, float radius, float thickness, uint32_t rgba)
{
    if (radius <= 0 || thickness <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    float t = std::min(thickness, radius);
    int segs = std::max(8, arc_segments(radius * f.xform.scale(), 2 * PI));
    f.local.clear();
    arc_local(f.local, cx, cy, radius - t * 0.5f, 0, 2 * PI, segs);
    f.local.pop_back();
    to_screen(f.xform, f.local, f.path);
    stroke(f, f.path, true, t * f.xform.scale(), pack(rgba, f.opacity));
}

inline void arc(Frame &f, float cx, float cy, float radius, float from, float to, float thickness, uint32_t rgba)
{
    if (radius <= 0 || thickness <= 0 || from == to || !rgba_visible(rgba) || clipped_out(f))
        return;
    to = std::clamp(to, from - 2 * PI, from + 2 * PI);
    bool full = std::fabs(to - from) >= 2 * PI - 1e-4f;
    int segs = std::max(2, arc_segments((radius + thickness * 0.5f) * f.xform.scale(), to - from));
    f.local.clear();
    arc_local(f.local, cx, cy, radius, from, to, segs);
    if (full)
        f.local.pop_back();
    to_screen(f.xform, f.local, f.path);
    stroke(f, f.path, full, thickness * f.xform.scale(), pack(rgba, f.opacity));
}

inline void polyline(Frame &f, const float *xy, uint32_t count, float thickness, uint32_t rgba, uint32_t flags)
{
    if (!xy || count < 2 || thickness <= 0 || !rgba_visible(rgba) || clipped_out(f))
        return;
    f.path.clear();
    for (uint32_t i = 0; i < count; i++) {
        Pt p;
        f.xform.apply(xy[2 * i], xy[2 * i + 1], p.x, p.y);
        if (f.path.empty() || std::fabs(p.x - f.path.back().x) + std::fabs(p.y - f.path.back().y) > 1e-3f)
            f.path.push_back(p);
    }
    bool closed = flags & SWOFTY_POLYLINE_CLOSED;
    if (closed && f.path.size() > 2 &&
        std::fabs(f.path[0].x - f.path.back().x) + std::fabs(f.path[0].y - f.path.back().y) <= 1e-3f)
        f.path.pop_back();
    stroke(f, f.path, closed, thickness * f.xform.scale(), pack(rgba, f.opacity));
}

inline void polygon(Frame &f, const float *xy, uint32_t count, uint32_t rgba)
{
    if (!xy || count < 3 || !rgba_visible(rgba) || clipped_out(f))
        return;
    f.path.resize(count);
    for (uint32_t i = 0; i < count; i++)
        f.xform.apply(xy[2 * i], xy[2 * i + 1], f.path[i].x, f.path[i].y);
    fill_convex(f, f.path, pack(rgba, f.opacity));
}

inline void polygon_concave(Frame &f, const float *xy, uint32_t count, uint32_t rgba)
{
    if (!xy || count < 3 || !rgba_visible(rgba) || clipped_out(f))
        return;
    f.path.clear();
    for (uint32_t i = 0; i < count; i++) {
        Pt p;
        f.xform.apply(xy[2 * i], xy[2 * i + 1], p.x, p.y);
        if (f.path.empty() || std::fabs(p.x - f.path.back().x) + std::fabs(p.y - f.path.back().y) > 1e-3f)
            f.path.push_back(p);
    }
    if (f.path.size() > 2 && std::fabs(f.path[0].x - f.path.back().x) + std::fabs(f.path[0].y - f.path.back().y) <= 1e-3f)
        f.path.pop_back();
    fill_concave(f, f.path, pack(rgba, f.opacity));
}

inline void image_nine(Frame &f, int slot, float iw, float ih, float x, float y, float w, float h, float l, float t, float r,
                       float b, float scale, uint32_t tint)
{
    if (slot < 0 || iw <= 0 || ih <= 0 || w <= 0 || h <= 0)
        return;
    l = std::clamp(l, 0.0f, iw);
    r = std::clamp(r, 0.0f, iw - l);
    t = std::clamp(t, 0.0f, ih);
    b = std::clamp(b, 0.0f, ih - t);
    float k = scale > 0 ? scale : 1;
    float dl = l * k, dr = r * k, dt = t * k, db = b * k;
    if (dl + dr > w) {
        float s = w / (dl + dr);
        dl *= s;
        dr *= s;
    }
    if (dt + db > h) {
        float s = h / (dt + db);
        dt *= s;
        db *= s;
    }
    float xs[4] = {x, x + dl, x + w - dr, x + w}, ys[4] = {y, y + dt, y + h - db, y + h};
    float us[4] = {0, l / iw, 1 - r / iw, 1}, vs[4] = {0, t / ih, 1 - b / ih, 1};
    for (int j = 0; j < 3; j++)
        for (int i = 0; i < 3; i++)
            image_uv(f, slot, xs[i], ys[j], xs[i + 1] - xs[i], ys[j + 1] - ys[j], us[i], vs[j], us[i + 1], vs[j + 1], tint);
}

}
