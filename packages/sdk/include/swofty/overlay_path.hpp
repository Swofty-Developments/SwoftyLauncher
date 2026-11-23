// SPDX-License-Identifier: MIT
#pragma once
#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <vector>

// Cubic bezier paths for the overlay: flatten_path turns a path into the points polygon_concave, polygon and polyline
// take (swofty/overlay.hpp). Header-only, no core calls.

namespace swofty::overlay {

// A vertex of a cubic bezier path: its position and its in and out tangents relative to it, as After Effects stores
// shape paths. A segment runs from a vertex along its out tangent to the next vertex's in tangent.
struct PathVertex {
    float x = 0, y = 0;
    float in_x = 0, in_y = 0;
    float out_x = 0, out_y = 0;
};

namespace detail {

inline float off_chord(float px, float py, float x0, float y0, float x1, float y1)
{
    float dx = x1 - x0, dy = y1 - y0, len = std::sqrt(dx * dx + dy * dy);
    return len > 1e-6f ? std::fabs((px - x0) * dy - (py - y0) * dx) / len : std::hypot(px - x0, py - y0);
}

inline void cubic(std::vector<float> &xy, float x0, float y0, float x1, float y1, float x2, float y2, float x3, float y3,
                  float tolerance, int depth)
{
    if (depth < 16 && (std::max)(off_chord(x1, y1, x0, y0, x3, y3), off_chord(x2, y2, x0, y0, x3, y3)) > tolerance) {
        float ax = (x0 + x1) / 2, ay = (y0 + y1) / 2, bx = (x1 + x2) / 2, by = (y1 + y2) / 2, cx = (x2 + x3) / 2,
              cy = (y2 + y3) / 2;
        float dx = (ax + bx) / 2, dy = (ay + by) / 2, ex = (bx + cx) / 2, ey = (by + cy) / 2;
        float mx = (dx + ex) / 2, my = (dy + ey) / 2;
        cubic(xy, x0, y0, ax, ay, dx, dy, mx, my, tolerance, depth + 1);
        cubic(xy, mx, my, ex, ey, cx, cy, x3, y3, tolerance, depth + 1);
        return;
    }
    xy.push_back(x3);
    xy.push_back(y3);
}

}

// Flatten a cubic bezier path into points (x0, y0, x1, y1, ...) for polygon_concave, polygon and polyline. A closed
// path also runs from the last vertex back to the first; its first point is not repeated at the end. The points stay
// within tolerance of the curve, in the path's units. Returns the number of points.
inline uint32_t flatten_path(const PathVertex *v, size_t count, bool closed, std::vector<float> &xy, float tolerance = 0.25f)
{
    xy.clear();
    if (!v || !count)
        return 0;
    tolerance = (std::max)(tolerance, 1e-3f);
    xy.push_back(v[0].x);
    xy.push_back(v[0].y);
    size_t segments = closed ? count : count - 1;
    for (size_t i = 0; i < segments; i++) {
        const PathVertex &a = v[i], &b = v[(i + 1) % count];
        detail::cubic(xy, a.x, a.y, a.x + a.out_x, a.y + a.out_y, b.x + b.in_x, b.y + b.in_y, b.x, b.y, tolerance, 0);
    }
    if (closed && xy.size() > 4 && xy[xy.size() - 2] == xy[0] && xy.back() == xy[1])
        xy.resize(xy.size() - 2);
    return uint32_t(xy.size() / 2);
}

}
