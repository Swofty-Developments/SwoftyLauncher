#pragma once
#include "core/hud/drawlist/fonts.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

namespace hud::drawlist {

constexpr uint32_t MAX_VERTS = 1u << 17;
constexpr size_t MAX_STACK = 64;
constexpr uint32_t MAX_CONCAVE_POINTS = 2048;
constexpr float SAFE_INSET = 0.035f;
constexpr float AA = 1.0f;
constexpr float PI = 3.14159265358979f;

enum Kind : uint32_t { SOLID = 0, IMAGE = 1, SDF = 2 };

constexpr int TEX_NONE = -1;
constexpr int TEX_FONT = 0;

struct Vertex {
    float x, y, u, v;
    uint32_t rgba;
    uint32_t mode;
};

struct Clip {
    float x0, y0, x1, y1;
    bool operator==(const Clip &) const = default;
};

struct Batch {
    uint32_t first, count;
    int tex;
    Clip clip;
};

struct Xform {
    float a = 1, b = 0, c = 0, d = 1, e = 0, f = 0;

    void apply(float x, float y, float &ox, float &oy) const
    {
        ox = a * x + c * y + e;
        oy = b * x + d * y + f;
    }

    float scale() const { return std::sqrt(std::fabs(a * d - b * c)); }

    bool plain() const { return a == 1 && b == 0 && c == 0 && d == 1; }

    Xform then(const Xform &m) const
    {
        return {a * m.a + c * m.b, b * m.a + d * m.b,     a * m.c + c * m.d,
                b * m.c + d * m.d, a * m.e + c * m.f + e, b * m.e + d * m.f + f};
    }
};

struct Pt {
    float x, y;
};

struct TextStyle {
    float px = 16;
    uint32_t rgba = 0xffffffff;
    uint32_t align = 0;
    float tracking = 0;
    float line_height = 1.2f;
    uint32_t outline_rgba = 0;
    float outline_px = 0;
    uint32_t shadow_rgba = 0;
    float shadow_dx = 0, shadow_dy = 0, shadow_blur = 0;
    float max_width = 0;
    Face *face = nullptr;
};

struct Line {
    size_t begin, end;
    float width;
};

struct Quad {
    float x0, y0, x1, y1, u0, v0, u1, v1;
    int tex = TEX_FONT;
    bool loaded = false;
};

struct LaidLine {
    float width;
    uint32_t first, count;
};

// A string laid out at one style: glyph quads relative to their line's pen origin on the baseline.
struct Layout {
    std::vector<Quad> quads;
    std::vector<LaidLine> lines;
    uint64_t used = 0;
};

constexpr size_t TEXT_CACHE_MAX = 512;
constexpr uint64_t TEXT_CACHE_KEEP = 120;

struct TextCache {
    std::unordered_map<std::string, Layout> map;
    uint64_t frame = 0;
    uint64_t hits = 0, misses = 0;
    std::string key;

    void trim()
    {
        if (map.size() <= TEXT_CACHE_MAX)
            return;
        for (auto it = map.begin(); it != map.end();)
            it = frame - it->second.used > TEXT_CACHE_KEEP ? map.erase(it) : std::next(it);
        if (map.size() > TEXT_CACHE_MAX)
            map.clear();
    }
};

struct Frame {
    float width = 0, height = 0;
    std::vector<Vertex> verts;
    std::vector<Batch> batches;
    Xform xform;
    Clip clip{};
    float opacity = 1;
    std::vector<Xform> xforms;
    std::vector<Clip> clips;
    std::vector<float> opacities;
    uint32_t xform_over = 0, clip_over = 0, opacity_over = 0;
    uint32_t dropped = 0;
    std::vector<Pt> path, local, ring_in, ring_out;
    std::vector<uint32_t> cols, tris;
    std::vector<Line> lines;
    TextCache text_cache;
    std::string owner;
};

inline void reset_state(Frame &f)
{
    f.xform = {};
    f.clip = {0, 0, f.width, f.height};
    f.opacity = 1;
    f.xforms.clear();
    f.clips.clear();
    f.opacities.clear();
    f.xform_over = f.clip_over = f.opacity_over = 0;
}

inline void begin(Frame &f, float width, float height)
{
    f.width = width;
    f.height = height;
    f.verts.clear();
    f.batches.clear();
    f.dropped = 0;
    f.text_cache.frame++;
    f.text_cache.trim();
    reset_state(f);
}

inline bool rgba_visible(uint32_t rgba) { return (rgba & 0xff) != 0; }

inline uint32_t pack(uint32_t rrggbbaa, float opacity)
{
    uint32_t r = (rrggbbaa >> 24) & 0xff, g = (rrggbbaa >> 16) & 0xff, b = (rrggbbaa >> 8) & 0xff, a = rrggbbaa & 0xff;
    if (opacity < 1)
        a = uint32_t(std::lround(float(a) * std::max(0.0f, opacity)));
    return r | (g << 8) | (b << 16) | (a << 24);
}

inline uint32_t clear_alpha(uint32_t packed) { return packed & 0x00ffffffu; }

inline uint32_t scale_alpha(uint32_t packed, float k)
{
    uint32_t a = uint32_t(std::lround(float(packed >> 24) * std::clamp(k, 0.0f, 1.0f)));
    return clear_alpha(packed) | a << 24;
}

inline bool clipped_out(const Frame &f) { return f.clip.x1 <= f.clip.x0 || f.clip.y1 <= f.clip.y0 || f.opacity <= 0; }

inline bool reserve(Frame &f, size_t n, int tex)
{
    if (f.verts.size() + n > MAX_VERTS) {
        f.dropped++;
        return false;
    }
    uint32_t at = uint32_t(f.verts.size());
    if (!f.batches.empty()) {
        Batch &b = f.batches.back();
        if (b.first + b.count == at && b.clip == f.clip && (b.tex == TEX_NONE || tex == TEX_NONE || b.tex == tex)) {
            if (b.tex == TEX_NONE)
                b.tex = tex;
            b.count += uint32_t(n);
            return true;
        }
    }
    f.batches.push_back({at, uint32_t(n), tex, f.clip});
    return true;
}

inline void vtx(Frame &f, float x, float y, uint32_t col, float u = 0, float v = 0, uint32_t mode = SOLID)
{
    f.verts.push_back({x, y, u, v, col, mode});
}

inline void tri(Frame &f, Pt a, Pt b, Pt c, uint32_t ca, uint32_t cb, uint32_t cc)
{
    vtx(f, a.x, a.y, ca);
    vtx(f, b.x, b.y, cb);
    vtx(f, c.x, c.y, cc);
}

inline void quad_flat(Frame &f, const Pt *p, uint32_t col)
{
    if (!reserve(f, 6, TEX_NONE))
        return;
    tri(f, p[0], p[1], p[3], col, col, col);
    tri(f, p[1], p[2], p[3], col, col, col);
}

inline void rect_points(const Xform &m, float x, float y, float w, float h, Pt *out)
{
    m.apply(x, y, out[0].x, out[0].y);
    m.apply(x + w, y, out[1].x, out[1].y);
    m.apply(x + w, y + h, out[2].x, out[2].y);
    m.apply(x, y + h, out[3].x, out[3].y);
}

}
