#pragma once
#include "core/hud/drawlist/frame.hpp"

#include <algorithm>
#include <cmath>

namespace hud::drawlist {

// ----- state stacks -----------------------------------------------------------------------------

inline void push_clip(Frame &f, float x, float y, float w, float h)
{
    if (f.clips.size() >= MAX_STACK) {
        f.clip_over++;
        return;
    }
    f.clips.push_back(f.clip);
    Pt p[4];
    rect_points(f.xform, x, y, std::max(w, 0.0f), std::max(h, 0.0f), p);
    Clip c{p[0].x, p[0].y, p[0].x, p[0].y};
    for (auto &q : p) {
        c.x0 = std::min(c.x0, q.x);
        c.y0 = std::min(c.y0, q.y);
        c.x1 = std::max(c.x1, q.x);
        c.y1 = std::max(c.y1, q.y);
    }
    f.clip = {std::max(c.x0, f.clip.x0), std::max(c.y0, f.clip.y0), std::min(c.x1, f.clip.x1), std::min(c.y1, f.clip.y1)};
}

inline void pop_clip(Frame &f)
{
    if (f.clip_over) {
        f.clip_over--;
        return;
    }
    if (f.clips.empty())
        return;
    f.clip = f.clips.back();
    f.clips.pop_back();
}

inline void push_transform(Frame &f, const float *m)
{
    if (f.xforms.size() >= MAX_STACK || !m) {
        f.xform_over++;
        return;
    }
    f.xforms.push_back(f.xform);
    f.xform = f.xform.then({m[0], m[1], m[2], m[3], m[4], m[5]});
}

inline void pop_transform(Frame &f)
{
    if (f.xform_over) {
        f.xform_over--;
        return;
    }
    if (f.xforms.empty())
        return;
    f.xform = f.xforms.back();
    f.xforms.pop_back();
}

inline void push_opacity(Frame &f, float opacity)
{
    if (f.opacities.size() >= MAX_STACK) {
        f.opacity_over++;
        return;
    }
    f.opacities.push_back(f.opacity);
    f.opacity *= std::clamp(opacity, 0.0f, 1.0f);
}

inline void pop_opacity(Frame &f)
{
    if (f.opacity_over) {
        f.opacity_over--;
        return;
    }
    if (f.opacities.empty())
        return;
    f.opacity = f.opacities.back();
    f.opacities.pop_back();
}

inline void safe_area(const Frame &f, float *x, float *y, float *w, float *h)
{
    float ix = std::round(f.width * SAFE_INSET), iy = std::round(f.height * SAFE_INSET);
    if (x)
        *x = ix;
    if (y)
        *y = iy;
    if (w)
        *w = std::max(0.0f, f.width - 2 * ix);
    if (h)
        *h = std::max(0.0f, f.height - 2 * iy);
}

inline float ui_scale(const Frame &f)
{
    if (f.width <= 0 || f.height <= 0)
        return 1;
    return std::min(f.width / 1920.0f, f.height / 1080.0f);
}

}
