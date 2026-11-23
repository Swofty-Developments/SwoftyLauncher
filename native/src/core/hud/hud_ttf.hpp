#pragma once
#include "core/hud/drawlist/fonts.hpp"
#include "core/hud/drawlist/registry.hpp"

#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-function"
#pragma GCC diagnostic ignored "-Wunused-parameter"
#pragma GCC diagnostic ignored "-Wsign-compare"
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#pragma GCC diagnostic ignored "-Wimplicit-fallthrough"
#endif
#define STBTT_STATIC
#define STB_TRUETYPE_IMPLEMENTATION
#include "../../../third_party/stb/stb_truetype.h"
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

// Fonts an addon loads for the overlay: TrueType and OpenType files (TrueType or CFF outlines) parsed with stb_truetype.
// Each glyph is rasterised the first time it is drawn into a signed distance field on an atlas page, a kept grey image
// in the overlay's registry. Texels hold 0.5 + d / (2 * SPREAD), d the signed distance in atlas pixels to the glyph's
// outline (positive inside), the same encoding as the built-in font.

namespace hud::ttf {

constexpr float ATLAS_EM = 64;
constexpr float SPREAD = 8;
constexpr uint32_t PAGE = 1024;
constexpr uint32_t MAX_PAGES = 8;
constexpr uint32_t GUTTER = 1;
constexpr size_t MAX_BYTES = size_t(32) << 20;
constexpr float FLATTEN_TOLERANCE = 0.05f;

struct Seg {
    float x0, y0, x1, y1;
};

// A glyph's signed distance field: w x h texels whose top-left sits at (x, y) atlas pixels from the pen on the baseline.
struct Sdf {
    int x = 0, y = 0, w = 0, h = 0;
    std::vector<uint8_t> texels;
};

inline int flatten_steps(float bend) { return std::clamp(int(std::ceil(std::sqrt(bend / FLATTEN_TOLERANCE))), 1, 32); }

// The outline of a glyph as line segments in atlas pixels, y down, every contour closed.
inline void outline(const stbtt_fontinfo &info, int glyph, float scale, std::vector<Seg> &segs)
{
    segs.clear();
    stbtt_vertex *v = nullptr;
    int n = stbtt_GetGlyphShape(&info, glyph, &v);
    float cx = 0, cy = 0, sx = 0, sy = 0;
    bool open = false;
    auto to = [&](float x, float y) {
        if (x != cx || y != cy)
            segs.push_back({cx, cy, x, y});
        cx = x;
        cy = y;
    };
    auto close = [&] {
        if (open)
            to(sx, sy);
        open = false;
    };
    for (int i = 0; i < n; i++) {
        float x = v[i].x * scale, y = -v[i].y * scale;
        float c0x = v[i].cx * scale, c0y = -v[i].cy * scale, c1x = v[i].cx1 * scale, c1y = -v[i].cy1 * scale;
        switch (v[i].type) {
        case STBTT_vmove:
            close();
            sx = cx = x;
            sy = cy = y;
            open = true;
            break;
        case STBTT_vline:
            to(x, y);
            break;
        case STBTT_vcurve: {
            float x0 = cx, y0 = cy;
            int k = flatten_steps(std::hypot(x0 - 2 * c0x + x, y0 - 2 * c0y + y) / 4);
            for (int s = 1; s <= k; s++) {
                float t = float(s) / float(k), u = 1 - t;
                to(u * u * x0 + 2 * u * t * c0x + t * t * x, u * u * y0 + 2 * u * t * c0y + t * t * y);
            }
            break;
        }
        case STBTT_vcubic: {
            float x0 = cx, y0 = cy;
            float bend =
                std::max(std::hypot(x0 - 2 * c0x + c1x, y0 - 2 * c0y + c1y), std::hypot(c0x - 2 * c1x + x, c0y - 2 * c1y + y));
            int k = flatten_steps(bend * 3 / 4);
            for (int s = 1; s <= k; s++) {
                float t = float(s) / float(k), u = 1 - t;
                float a = u * u * u, b = 3 * u * u * t, c = 3 * u * t * t, d = t * t * t;
                to(a * x0 + b * c0x + c * c1x + d * x, a * y0 + b * c0y + c * c1y + d * y);
            }
            break;
        }
        }
    }
    close();
    if (v)
        stbtt_FreeShape(&info, v);
}

inline float segment_distance2(const Seg &s, float px, float py)
{
    float dx = s.x1 - s.x0, dy = s.y1 - s.y0, len2 = dx * dx + dy * dy;
    float t = len2 > 0 ? std::clamp(((px - s.x0) * dx + (py - s.y0) * dy) / len2, 0.0f, 1.0f) : 0;
    float ex = s.x0 + t * dx - px, ey = s.y0 + t * dy - py;
    return ex * ex + ey * ey;
}

// The signed distance field of closed segments, spread texels of padding on each side; inside is non-zero winding.
inline Sdf distance_field(const std::vector<Seg> &segs, float spread)
{
    Sdf out;
    if (segs.empty())
        return out;
    float bx0 = segs[0].x0, by0 = segs[0].y0, bx1 = bx0, by1 = by0;
    for (const Seg &s : segs) {
        bx0 = std::min({bx0, s.x0, s.x1}), by0 = std::min({by0, s.y0, s.y1});
        bx1 = std::max({bx1, s.x0, s.x1}), by1 = std::max({by1, s.y0, s.y1});
    }
    int pad = int(std::ceil(spread));
    out.x = int(std::floor(bx0)) - pad;
    out.y = int(std::floor(by0)) - pad;
    out.w = int(std::ceil(bx1)) + pad - out.x;
    out.h = int(std::ceil(by1)) + pad - out.y;
    out.texels.assign(size_t(out.w) * size_t(out.h), 0);
    std::vector<std::pair<float, int>> crossings;
    std::vector<const Seg *> near;
    const float reach = spread + 1;
    for (int j = 0; j < out.h; j++) {
        float py = float(out.y + j) + 0.5f;
        crossings.clear();
        near.clear();
        for (const Seg &s : segs) {
            if ((s.y0 <= py) != (s.y1 <= py))
                crossings.push_back({s.x0 + (py - s.y0) / (s.y1 - s.y0) * (s.x1 - s.x0), s.y1 > s.y0 ? 1 : -1});
            if (std::min(s.y0, s.y1) - reach <= py && std::max(s.y0, s.y1) + reach >= py)
                near.push_back(&s);
        }
        std::sort(crossings.begin(), crossings.end());
        size_t next = 0;
        int winding = 0;
        for (int i = 0; i < out.w; i++) {
            float px = float(out.x + i) + 0.5f;
            while (next < crossings.size() && crossings[next].first < px)
                winding += crossings[next++].second;
            float best = reach * reach;
            for (const Seg *s : near)
                best = std::min(best, segment_distance2(*s, px, py));
            float d = std::sqrt(best) * (winding != 0 ? 1.0f : -1.0f);
            float v = std::clamp(0.5f + d / (2 * spread), 0.0f, 1.0f);
            out.texels[size_t(j) * size_t(out.w) + size_t(i)] = uint8_t(std::lround(v * 255));
        }
    }
    return out;
}

inline std::string utf16be_to_utf8(const char *s, int len)
{
    std::string out;
    for (int i = 0; i + 1 < len; i += 2) {
        uint32_t c = uint32_t(uint8_t(s[i])) << 8 | uint8_t(s[i + 1]);
        if (c < 0x80) {
            out += char(c);
        } else if (c < 0x800) {
            out += char(0xc0 | c >> 6);
            out += char(0x80 | (c & 0x3f));
        } else {
            out += char(0xe0 | c >> 12);
            out += char(0x80 | (c >> 6 & 0x3f));
            out += char(0x80 | (c & 0x3f));
        }
    }
    return out;
}

class TtfFace final : public drawlist::Face {
public:
    // Parses font bytes. Atlas pages go into reg (under lock) as kept grey images named page_prefix + page number, in
    // slots up to max_slot. nullptr with why set when the bytes are not a font stb_truetype reads.
    static std::shared_ptr<TtfFace> open(std::string bytes, drawlist::Registry &reg, std::mutex &lock, std::string page_prefix,
                                         int max_slot, std::string &why)
    {
        if (bytes.empty())
            return why = "the font is empty", nullptr;
        if (bytes.size() > MAX_BYTES)
            return why = "the font is over 32 MB", nullptr;
        auto face = std::shared_ptr<TtfFace>(new TtfFace(std::move(bytes), reg, lock, std::move(page_prefix), max_slot));
        const auto *data = reinterpret_cast<const unsigned char *>(face->data_.data());
        int offset = face->data_.size() >= 12 ? stbtt_GetFontOffsetForIndex(data, 0) : -1;
        if (offset < 0 || !stbtt_InitFont(&face->info_, data, offset) || face->info_.numGlyphs <= 0)
            return why = "not a TrueType or OpenType font", nullptr;
        face->measure();
        static std::atomic<uint64_t> serials{1};
        face->serial = serials++;
        return face;
    }

    uint32_t index(uint32_t cp) const override { return cp > 0x10ffff ? 0 : uint32_t(stbtt_FindGlyphIndex(&info_, int(cp))); }

    float advance(uint32_t glyph) const override
    {
        int adv = 0, lsb = 0;
        stbtt_GetGlyphHMetrics(&info_, int(glyph), &adv, &lsb);
        return float(adv) * em_;
    }

    float kern(uint32_t left, uint32_t right) const override
    {
        return info_.kern || info_.gpos ? float(stbtt_GetGlyphKernAdvance(&info_, int(left), int(right))) * em_ : 0;
    }

    drawlist::FaceGlyph glyph(uint32_t glyph) override
    {
        std::lock_guard<std::mutex> g(mu_);
        auto it = glyphs_.find(glyph);
        if (it != glyphs_.end())
            return it->second;
        drawlist::FaceGlyph out = rasterise(glyph);
        glyphs_.emplace(glyph, out);
        return out;
    }

    // Rasterises the glyphs of the codepoints from..to the font has.
    void warm(uint32_t from, uint32_t to)
    {
        for (uint32_t cp = from; cp <= to; cp++)
            if (uint32_t g = index(cp))
                glyph(g);
    }

    std::string name() const
    {
        int len = 0;
        const char *s = stbtt_GetFontNameString(&info_, &len, STBTT_PLATFORM_ID_MICROSOFT, STBTT_MS_EID_UNICODE_BMP,
                                                STBTT_MS_LANG_ENGLISH, 4);
        if (s && len > 0)
            return utf16be_to_utf8(s, len);
        s = stbtt_GetFontNameString(&info_, &len, STBTT_PLATFORM_ID_MAC, STBTT_MAC_EID_ROMAN, STBTT_MAC_LANG_ENGLISH, 4);
        return s && len > 0 ? std::string(s, size_t(len)) : std::string("unnamed font");
    }

    size_t glyph_count()
    {
        std::lock_guard<std::mutex> g(mu_);
        return glyphs_.size();
    }

    size_t page_count()
    {
        std::lock_guard<std::mutex> g(mu_);
        return slots_.size();
    }

    bool cff() const { return info_.cff.size > 0; }

private:
    TtfFace(std::string bytes, drawlist::Registry &reg, std::mutex &lock, std::string page_prefix, int max_slot)
        : data_(std::move(bytes)), reg_(reg), lock_(lock), prefix_(std::move(page_prefix)), max_slot_(max_slot)
    {
        atlas_em = ATLAS_EM;
        spread = SPREAD;
    }

    float box_top(uint32_t cp, bool bottom) const
    {
        int x0, y0, x1, y1;
        if (!stbtt_GetCodepointBox(&info_, int(cp), &x0, &y0, &x1, &y1))
            return -1;
        return float(bottom ? -y0 : y1) * em_;
    }

    void measure()
    {
        em_ = stbtt_ScaleForMappingEmToPixels(&info_, 1);
        int ascent = 0, descent = 0, gap = 0;
        stbtt_GetFontVMetrics(&info_, &ascent, &descent, &gap);
        float cap = box_top('H', false);
        if (cap <= 0)
            cap = float(ascent) * em_ * 0.7f;
        float below = -1;
        for (uint32_t cp : {'g', 'j', 'p', 'q', 'y'})
            below = std::max(below, box_top(cp, true));
        if (below < 0)
            below = float(-descent) * em_;
        baseline = (1 - cap - below) / 2 + cap;
    }

    bool place(uint32_t w, uint32_t h, int &slot, uint32_t &x, uint32_t &y)
    {
        if (w + GUTTER > PAGE || h + GUTTER > PAGE)
            return false;
        if (!slots_.empty() && pen_x_ + w + GUTTER > PAGE) {
            pen_x_ = 0;
            shelf_y_ += shelf_h_;
            shelf_h_ = 0;
        }
        if (slots_.empty() || shelf_y_ + h + GUTTER > PAGE) {
            if (slots_.size() >= MAX_PAGES || full_)
                return false;
            int at;
            {
                std::lock_guard<std::mutex> g(lock_);
                at = reg_.add_page(prefix_ + std::to_string(slots_.size()), PAGE, PAGE);
            }
            if (at > max_slot_) {
                full_ = true;
                return false;
            }
            slots_.push_back(at);
            pen_x_ = shelf_y_ = shelf_h_ = 0;
        }
        slot = slots_.back();
        x = pen_x_ + GUTTER;
        y = shelf_y_ + GUTTER;
        pen_x_ += w + GUTTER;
        shelf_h_ = std::max(shelf_h_, h + GUTTER);
        return true;
    }

    drawlist::FaceGlyph rasterise(uint32_t glyph)
    {
        drawlist::FaceGlyph out;
        outline(info_, int(glyph), em_ * ATLAS_EM, segs_);
        Sdf sdf = distance_field(segs_, SPREAD);
        uint32_t x = 0, y = 0;
        if (sdf.texels.empty() || !place(uint32_t(sdf.w), uint32_t(sdf.h), out.slot, x, y))
            return out.slot = -1, out;
        {
            std::lock_guard<std::mutex> g(lock_);
            reg_.blit(out.slot, x, y, uint32_t(sdf.w), uint32_t(sdf.h), sdf.texels.data());
        }
        out.x0 = float(sdf.x) / ATLAS_EM;
        out.y0 = float(sdf.y) / ATLAS_EM;
        out.x1 = float(sdf.x + sdf.w) / ATLAS_EM;
        out.y1 = float(sdf.y + sdf.h) / ATLAS_EM;
        out.u0 = float(x) / float(PAGE);
        out.v0 = float(y) / float(PAGE);
        out.u1 = float(x + uint32_t(sdf.w)) / float(PAGE);
        out.v1 = float(y + uint32_t(sdf.h)) / float(PAGE);
        return out;
    }

    std::string data_;
    stbtt_fontinfo info_{};
    float em_ = 0;
    drawlist::Registry &reg_;
    std::mutex &lock_;
    std::string prefix_;
    int max_slot_;
    std::mutex mu_;
    std::unordered_map<uint32_t, drawlist::FaceGlyph> glyphs_;
    std::vector<Seg> segs_;
    std::vector<int> slots_;
    uint32_t pen_x_ = 0, shelf_y_ = 0, shelf_h_ = 0;
    bool full_ = false;
};

}
