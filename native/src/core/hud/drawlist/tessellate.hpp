#pragma once
#include "core/hud/drawlist/frame.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <vector>

namespace hud::drawlist {

inline float signed_area(const std::vector<Pt> &p)
{
    float a = 0;
    for (size_t i = 0, n = p.size(); i < n; i++) {
        const Pt &u = p[i], &v = p[(i + 1) % n];
        a += u.x * v.y - v.x * u.y;
    }
    return a * 0.5f;
}

inline Pt edge_normal(Pt a, Pt b)
{
    float dx = b.x - a.x, dy = b.y - a.y, len = std::sqrt(dx * dx + dy * dy);
    if (len <= 1e-6f)
        return {0, 0};
    return {dy / len, -dx / len};
}

inline Pt miter(Pt n0, Pt n1)
{
    Pt m{(n0.x + n1.x) * 0.5f, (n0.y + n1.y) * 0.5f};
    float d2 = m.x * m.x + m.y * m.y;
    if (d2 > 1e-6f) {
        float inv = std::min(1.0f / d2, 100.0f);
        m.x *= inv;
        m.y *= inv;
    }
    return m;
}

// Fills a convex polygon given in framebuffer pixels: an opaque fan inset half a pixel and a fringe
// that fades out half a pixel outside. cols holds one packed colour per point.
inline void fill_convex(Frame &f, const std::vector<Pt> &pts, const std::vector<uint32_t> &cols)
{
    size_t n = pts.size();
    if (n < 3 || cols.size() != n)
        return;
    float area = signed_area(pts);
    if (std::fabs(area) < 1e-4f)
        return;
    if (!reserve(f, 3 * (n - 2) + 6 * n, TEX_NONE))
        return;
    float k = AA * 0.5f * (area > 0 ? 1.0f : -1.0f);
    std::vector<Pt> &in = f.ring_in, &out = f.ring_out;
    in.resize(n);
    out.resize(n);
    for (size_t i = 0; i < n; i++) {
        Pt m = miter(edge_normal(pts[(i + n - 1) % n], pts[i]), edge_normal(pts[i], pts[(i + 1) % n]));
        in[i] = {pts[i].x - m.x * k, pts[i].y - m.y * k};
        out[i] = {pts[i].x + m.x * k, pts[i].y + m.y * k};
    }
    for (size_t i = 1; i + 1 < n; i++)
        tri(f, in[0], in[i], in[i + 1], cols[0], cols[i], cols[i + 1]);
    for (size_t i = 0; i < n; i++) {
        size_t j = (i + 1) % n;
        uint32_t ci = cols[i], cj = cols[j];
        tri(f, in[i], out[i], out[j], ci, clear_alpha(ci), clear_alpha(cj));
        tri(f, in[i], out[j], in[j], ci, clear_alpha(cj), cj);
    }
}

inline void fill_convex(Frame &f, const std::vector<Pt> &pts, uint32_t col)
{
    f.cols.assign(pts.size(), col);
    fill_convex(f, pts, f.cols);
}

// Strokes a path given in framebuffer pixels: an opaque core of thickness - 1 with a one-pixel fringe
// on each side; a path thinner than a pixel is a faded hairline.
inline void stroke(Frame &f, const std::vector<Pt> &pts, bool closed, float thickness, uint32_t col)
{
    size_t n = pts.size();
    if (n < 2 || thickness <= 0)
        return;
    if (closed && n < 3)
        closed = false;
    size_t segs = closed ? n : n - 1;
    bool thick = thickness > AA;
    if (!reserve(f, segs * (thick ? 18 : 12), TEX_NONE))
        return;
    std::vector<Pt> &dm = f.ring_in;
    dm.resize(n);
    for (size_t i = 0; i < n; i++) {
        Pt n0 = !closed && i == 0 ? edge_normal(pts[0], pts[1]) : edge_normal(pts[(i + n - 1) % n], pts[i]);
        Pt n1 = !closed && i == n - 1 ? edge_normal(pts[n - 2], pts[n - 1]) : edge_normal(pts[i], pts[(i + 1) % n]);
        dm[i] = miter(n0, n1);
    }
    uint32_t core = thick ? col : scale_alpha(col, thickness), edge = clear_alpha(col);
    float half = thick ? (thickness - AA) * 0.5f : 0;
    auto at = [&](size_t k, float off) { return Pt{pts[k].x + dm[k].x * off, pts[k].y + dm[k].y * off}; };
    for (size_t s = 0; s < segs; s++) {
        size_t i = s, j = (s + 1) % n;
        if (thick) {
            Pt ao = at(i, half + AA), ac = at(i, half), bc = at(i, -half), bo = at(i, -half - AA);
            Pt co = at(j, half + AA), cc = at(j, half), dc = at(j, -half), d_o = at(j, -half - AA);
            tri(f, ao, co, cc, edge, edge, core);
            tri(f, ao, cc, ac, edge, core, core);
            tri(f, ac, cc, dc, core, core, core);
            tri(f, ac, dc, bc, core, core, core);
            tri(f, bc, dc, d_o, core, core, edge);
            tri(f, bc, d_o, bo, core, edge, edge);
        } else {
            Pt a = pts[i], b = pts[j], ao = at(i, AA), bo = at(j, AA), ai = at(i, -AA), bi = at(j, -AA);
            tri(f, ao, bo, b, edge, edge, core);
            tri(f, ao, b, a, edge, core, core);
            tri(f, a, b, bi, core, core, edge);
            tri(f, a, bi, ai, core, edge, edge);
        }
    }
}

inline int arc_segments(float radius_px, float angle)
{
    if (radius_px <= 0.5f)
        return 1;
    float step = 2 * std::acos(std::clamp(1 - 0.25f / radius_px, -1.0f, 1.0f));
    int n = int(std::ceil(std::fabs(angle) / std::max(step, 0.02f)));
    return std::clamp(n, 1, 256);
}

inline void arc_local(std::vector<Pt> &out, float cx, float cy, float r, float a0, float a1, int segs)
{
    for (int i = 0; i <= segs; i++) {
        float a = a0 + (a1 - a0) * float(i) / float(segs);
        out.push_back({cx + r * std::cos(a), cy + r * std::sin(a)});
    }
}

inline void rounded_local(std::vector<Pt> &out, float x, float y, float w, float h, float r, float scale)
{
    out.clear();
    r = std::clamp(r, 0.0f, std::min(w, h) * 0.5f);
    if (r * scale <= 0.5f) {
        out.assign({{x, y}, {x + w, y}, {x + w, y + h}, {x, y + h}});
        return;
    }
    int segs = arc_segments(r * scale, PI / 2);
    arc_local(out, x + r, y + r, r, PI, 1.5f * PI, segs);
    arc_local(out, x + w - r, y + r, r, 1.5f * PI, 2 * PI, segs);
    arc_local(out, x + w - r, y + h - r, r, 0, 0.5f * PI, segs);
    arc_local(out, x + r, y + h - r, r, 0.5f * PI, PI, segs);
    auto same = [](Pt a, Pt b) { return std::fabs(a.x - b.x) + std::fabs(a.y - b.y) <= 1e-4f; };
    out.erase(std::unique(out.begin(), out.end(), same), out.end());
    if (out.size() > 1 && same(out.front(), out.back()))
        out.pop_back();
}

inline void to_screen(const Xform &m, const std::vector<Pt> &local, std::vector<Pt> &out)
{
    out.resize(local.size());
    for (size_t i = 0; i < local.size(); i++)
        m.apply(local[i].x, local[i].y, out[i].x, out[i].y);
}

}
