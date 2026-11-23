#pragma once
#include "core/hud/drawlist/frame.hpp"
#include "core/hud/hud_font.hpp"
#include "swofty_api.h"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <string_view>
#include <vector>

namespace hud::drawlist {

// ----- text -------------------------------------------------------------------------------------

// Where a codepoint's glyph comes from: the style's loaded font when it has the glyph, else the built-in font
// (which draws a codepoint it lacks as '?').
struct Pick {
    Face *face = nullptr;
    uint32_t index = 0;
    const font::Glyph *builtin = nullptr;
};

inline Pick pick(const TextStyle &s, uint32_t cp)
{
    if (s.face) {
        uint32_t gi = s.face->index(cp);
        if (!gi && (cp == '\t' || cp == 0xa0))
            gi = s.face->index(' ');
        if (gi)
            return {s.face, gi, nullptr};
    }
    return {nullptr, 0, font::glyph(cp)};
}

inline float advance(const Pick &p) { return p.face ? p.face->advance(p.index) : p.builtin ? p.builtin->advance : 0; }

inline float kern(const Pick &left, uint32_t left_cp, const Pick &right, uint32_t right_cp)
{
    if (left.face || right.face)
        return left.face == right.face ? left.face->kern(left.index, right.index) : 0;
    return font::kern(left_cp, right_cp);
}

inline float baseline(const TextStyle &s) { return s.face ? s.face->baseline : font::BASELINE; }

// Splits text into lines at '\n' and, with a max width, at the last space that keeps a line inside it
// (mid-word when a word alone is wider). A line's width is its pen advance.
inline void layout(const TextStyle &s, std::string_view text, std::vector<Line> &lines)
{
    lines.clear();
    size_t i = 0, begin = 0, space = std::string_view::npos, prev_end = 0;
    float width = 0, at_space = 0;
    uint32_t prev = 0;
    Pick prev_pick;
    auto cut = [&](size_t end, float w, size_t next) {
        lines.push_back({begin, end, w});
        begin = i = next;
        width = 0;
        prev = 0;
        space = std::string_view::npos;
    };
    while (i < text.size()) {
        size_t at = i;
        uint32_t cp = font::decode(text, i);
        if (cp == '\n') {
            cut(at, width, i);
            continue;
        }
        if (cp < 0x20)
            continue;
        Pick p = pick(s, cp);
        float k = prev ? kern(prev_pick, prev, p, cp) * s.px + s.tracking : 0;
        float adv = advance(p) * s.px;
        if (s.max_width > 0 && width + k + adv > s.max_width && at > begin) {
            if (cp == ' ')
                cut(at, width, i);
            else if (space != std::string_view::npos)
                cut(space, at_space, space + 1);
            else
                cut(prev_end, width, prev_end);
            continue;
        }
        if (cp == ' ') {
            space = at;
            at_space = width;
        }
        width += k + adv;
        prev = cp;
        prev_pick = p;
        prev_end = i;
    }
    lines.push_back({begin, text.size(), width});
}

inline float line_step(const TextStyle &s) { return (s.line_height > 0 ? s.line_height : 1.2f) * s.px; }

inline void measure(const TextStyle &s, std::string_view text, float *width, float *height)
{
    static thread_local std::vector<Line> lines;
    float w = 0;
    size_t n = 1;
    if (s.px > 0) {
        layout(s, text, lines);
        for (auto &l : lines)
            w = std::max(w, l.width);
        n = lines.size();
    }
    if (width)
        *width = w;
    if (height)
        *height = s.px > 0 ? s.px + float(n - 1) * line_step(s) : 0;
}

inline void measure(float font_size, std::string_view s, float *width, float *height)
{
    TextStyle st;
    st.px = font_size;
    measure(st, s, width, height);
}

inline uint32_t sdf_mode(float dilate, float soft)
{
    uint32_t d = uint32_t(std::clamp(dilate, 0.0f, 0.5f) * 510.0f + 0.5f);
    uint32_t s = uint32_t(std::clamp(soft, 0.0f, 31.0f) * 8.0f + 0.5f);
    return SDF | d << 8 | s << 16;
}

inline void lay_out(const TextStyle &s, std::string_view text, std::vector<Line> &lines, Layout &out)
{
    layout(s, text, lines);
    out.quads.clear();
    out.lines.clear();
    const float iw = 1.0f / float(font::ATLAS_W), ih = 1.0f / float(font::ATLAS_H);
    for (const Line &l : lines) {
        LaidLine ll{l.width, uint32_t(out.quads.size()), 0};
        float pen = 0;
        uint32_t prev = 0;
        Pick prev_pick;
        for (size_t i = l.begin; i < l.end;) {
            uint32_t cp = font::decode(text, i);
            if (cp < 0x20)
                continue;
            Pick p = pick(s, cp);
            if (prev)
                pen += kern(prev_pick, prev, p, cp) * s.px + s.tracking;
            prev = cp;
            prev_pick = p;
            if (p.face) {
                FaceGlyph g = p.face->glyph(p.index);
                if (g.slot >= 0 && g.x1 > g.x0 && g.y1 > g.y0)
                    out.quads.push_back({pen + g.x0 * s.px, g.y0 * s.px, pen + g.x1 * s.px, g.y1 * s.px, g.u0, g.v0, g.u1, g.v1,
                                         g.slot + 1, true});
            } else if (const font::Glyph *g = p.builtin; g && g->u1 > g->u0 && g->v1 > g->v0) {
                out.quads.push_back({pen + g->x0 * s.px, g->y0 * s.px, pen + g->x1 * s.px, g->y1 * s.px, g->u0 * iw, g->v0 * ih,
                                     g->u1 * iw, g->v1 * ih, TEX_FONT, false});
            }
            pen += advance(p) * s.px;
        }
        ll.count = uint32_t(out.quads.size()) - ll.first;
        out.lines.push_back(ll);
    }
}

inline const Layout &cached_layout(Frame &f, const TextStyle &s, std::string_view text)
{
    TextCache &c = f.text_cache;
    c.key.assign(text);
    const float params[4] = {s.px, s.tracking, s.max_width, s.line_height};
    c.key.append(reinterpret_cast<const char *>(params), sizeof params);
    const uint64_t face = s.face ? s.face->serial : 0;
    c.key.append(reinterpret_cast<const char *>(&face), sizeof face);
    auto [it, fresh] = c.map.try_emplace(c.key);
    if (fresh) {
        c.misses++;
        lay_out(s, text, f.lines, it->second);
    } else {
        c.hits++;
    }
    it->second.used = c.frame;
    return it->second;
}

// The SDF mode of a pass for glyphs of the built-in font and for glyphs of a loaded font, whose atlases differ in scale.
struct RunModes {
    uint32_t builtin, loaded;
};

inline void glyph_run(Frame &f, const TextStyle &s, const Layout &lay, float top, float x, float dx, float dy, uint32_t col,
                      RunModes modes)
{
    float step = line_step(s);
    bool snap = f.xform.plain();
    uint32_t h = s.align & 0x0f;
    for (size_t li = 0; li < lay.lines.size(); li++) {
        const LaidLine &l = lay.lines[li];
        float pen = x - (h == SWOFTY_TEXT_CENTER ? l.width * 0.5f : h == SWOFTY_TEXT_RIGHT ? l.width : 0) + dx;
        float base = top + float(li) * step + baseline(s) * s.px + dy;
        if (snap) {
            pen = std::round(pen + f.xform.e) - f.xform.e;
            base = std::round(base + f.xform.f) - f.xform.f;
        }
        const uint32_t end = l.first + l.count;
        for (uint32_t run = l.first; run < end;) {
            uint32_t stop = run + 1;
            while (stop < end && lay.quads[stop].tex == lay.quads[run].tex)
                stop++;
            if (reserve(f, size_t(stop - run) * 6, lay.quads[run].tex))
                for (uint32_t q = run; q < stop; q++) {
                    const Quad &g = lay.quads[q];
                    Pt p[4];
                    rect_points(f.xform, pen + g.x0, base + g.y0, g.x1 - g.x0, g.y1 - g.y0, p);
                    float u[4] = {g.u0, g.u1, g.u1, g.u0}, v[4] = {g.v0, g.v0, g.v1, g.v1};
                    uint32_t mode = g.loaded ? modes.loaded : modes.builtin;
                    for (int k : {0, 1, 3, 1, 2, 3})
                        vtx(f, p[k].x, p[k].y, col, u[k], v[k], mode);
                }
            run = stop;
        }
    }
}

inline void text(Frame &f, float x, float y, const TextStyle &s, std::string_view text)
{
    if (s.px <= 0 || text.empty() || clipped_out(f))
        return;
    const Layout &lay = cached_layout(f, s, text);
    float height = s.px + float(lay.lines.size() - 1) * line_step(s);
    uint32_t v = s.align & 0xf0;
    float top = v == SWOFTY_TEXT_MIDDLE     ? y - height * 0.5f
                : v == SWOFTY_TEXT_BASELINE ? y - baseline(s) * s.px
                : v == SWOFTY_TEXT_BOTTOM   ? y - height
                                            : y;
    auto per_px = [&](float atlas_em, float spread) {
        float scale = s.px / atlas_em * f.xform.scale();
        return scale > 0 ? 1.0f / (2 * spread * scale) : 0.0f;
    };
    const float per_builtin = per_px(font::ATLAS_EM, font::SPREAD);
    const float per_loaded = s.face ? per_px(s.face->atlas_em, s.face->spread) : per_builtin;
    auto modes = [&](float dilate_px, float soft) {
        return RunModes{sdf_mode(dilate_px * per_builtin, soft), sdf_mode(dilate_px * per_loaded, soft)};
    };
    bool outline = s.outline_px > 0 && rgba_visible(s.outline_rgba);
    if (rgba_visible(s.shadow_rgba))
        glyph_run(f, s, lay, top, x, s.shadow_dx, s.shadow_dy, pack(s.shadow_rgba, f.opacity),
                  modes(outline ? s.outline_px : 0, s.shadow_blur));
    if (outline)
        glyph_run(f, s, lay, top, x, 0, 0, pack(s.outline_rgba, f.opacity), modes(s.outline_px, 0));
    if (rgba_visible(s.rgba))
        glyph_run(f, s, lay, top, x, 0, 0, pack(s.rgba, f.opacity), modes(0, 0));
}

inline void text(Frame &f, float x, float y, float font_size, uint32_t rgba, std::string_view s)
{
    if (font_size <= 0 || !rgba_visible(rgba) || s.empty())
        return;
    TextStyle st;
    st.px = font_size;
    st.rgba = rgba;
    text(f, x, y, st, s);
}

}
