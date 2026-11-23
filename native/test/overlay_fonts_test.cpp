// Host unit test for the overlay's loaded fonts (native/src/core/hud/hud_ttf.hpp) and concave fills: parsing TrueType
// and CFF fonts, the signed distance field glyph atlas, measure and layout with a loaded font, the fallback to the
// built-in font, the ear-clipping triangulator and swofty/overlay_path.hpp. Arguments: a TrueType font, a CFF font and,
// optionally, a folder to write a CPU-rasterised scene to (fonts-scene.png).

#include "core/hud/hud_drawlist.hpp"
#include "core/hud/hud_font_atlas.hpp"
#include "core/hud/hud_image.hpp"
#include "core/hud/hud_png.hpp"
#include "core/hud/hud_ttf.hpp"

#include <swofty/overlay_path.hpp>

#include <chrono>
#include <cmath>
#include <cstdio>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

static int failures;

#define CHECK(cond, msg)                                                                                                         \
    do {                                                                                                                         \
        if (cond) {                                                                                                              \
            printf("ok  : %s\n", msg);                                                                                           \
        } else {                                                                                                                 \
            printf("FAIL: %s\n", msg);                                                                                           \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

using hud::drawlist::Pt;
using hud::ttf::TtfFace;

static std::string read(const char *path)
{
    std::ifstream f(path, std::ios::binary);
    std::stringstream s;
    s << f.rdbuf();
    return s.str();
}

static bool near(float a, float b, float eps = 1e-3f) { return std::fabs(a - b) <= eps; }

struct Fonts {
    hud::drawlist::Registry reg;
    std::mutex lock;
    std::shared_ptr<TtfFace> ttf, cff;
};

static std::shared_ptr<TtfFace> open(Fonts &fs, const std::string &bytes, const char *prefix, int max_slot = 254)
{
    std::string why;
    return TtfFace::open(bytes, fs.reg, fs.lock, prefix, max_slot, why);
}

static void test_loader(Fonts &fs, const std::string &ttf, const std::string &cff)
{
    fs.ttf = open(fs, ttf, "ttf:");
    fs.cff = open(fs, cff, "cff:");
    CHECK(fs.ttf && !fs.ttf->cff() && fs.ttf->name().find("DejaVu") != std::string::npos,
          "a TrueType font parses and names itself");
    CHECK(fs.cff && fs.cff->cff() && fs.cff->name() == "Swofty Test CFF", "an OpenType font with CFF outlines parses");
    std::string why;
    CHECK(!TtfFace::open("this is not a font file at all", fs.reg, fs.lock, "x:", 254, why) &&
              why == "not a TrueType or OpenType font",
          "bytes that are not a font are refused");
    CHECK(!TtfFace::open("", fs.reg, fs.lock, "x:", 254, why) && why == "the font is empty", "empty bytes are refused");
    CHECK(fs.ttf && fs.cff && fs.ttf->serial != fs.cff->serial && fs.ttf->serial && fs.cff->serial,
          "every loaded font has its own serial for the layout cache");
    CHECK(fs.reg.font_count() == 0 && fs.ttf->page_count() == 0, "opening a font rasterises nothing");
}

static bool overlaps(const hud::drawlist::FaceGlyph &a, const hud::drawlist::FaceGlyph &b)
{
    return a.slot == b.slot && a.u0 < b.u1 && b.u0 < a.u1 && a.v0 < b.v1 && b.v0 < a.v1;
}

static uint8_t texel(const hud::drawlist::Image &im, float u, float v)
{
    uint32_t x = std::min(uint32_t(u * float(im.width)), im.width - 1),
             y = std::min(uint32_t(v * float(im.height)), im.height - 1);
    return im.rgba[size_t(y) * im.width + x];
}

static void test_atlas(Fonts &fs)
{
    TtfFace &f = *fs.ttf;
    auto t0 = std::chrono::steady_clock::now();
    hud::drawlist::FaceGlyph bar = f.glyph(f.index('I'));
    const hud::drawlist::Image *page = fs.reg.image(bar.slot);
    CHECK(bar.slot >= 0 && page && page->channels == 1 && page->keep && page->width == hud::ttf::PAGE &&
              page->height == hud::ttf::PAGE,
          "the first glyph opens a kept 1024x1024 grey atlas page in the registry");
    CHECK(near((bar.x1 - bar.x0) * hud::ttf::ATLAS_EM, (bar.u1 - bar.u0) * float(hud::ttf::PAGE), 1e-2f) &&
              near((bar.y1 - bar.y0) * hud::ttf::ATLAS_EM, (bar.v1 - bar.v0) * float(hud::ttf::PAGE), 1e-2f),
          "a glyph quad and its atlas rect have the same size");
    CHECK(bar.y0 < -0.6f && bar.y1 > 0 && bar.y1 < 0.3f, "a capital sits above the baseline, its box padded by the spread");
    float cu = (bar.u0 + bar.u1) / 2, cv = (bar.v0 + bar.v1) / 2;
    CHECK(page && texel(*page, cu, cv) > 150 && texel(*page, bar.u0, bar.v0) < 20,
          "the field is high inside the stem and zero a spread outside it");
    uint32_t gen = page ? page->generation : 0;
    hud::drawlist::FaceGlyph again = f.glyph(f.index('I'));
    CHECK(again.u0 == bar.u0 && again.slot == bar.slot && fs.reg.image(bar.slot)->generation == gen,
          "a glyph is rasterised once");
    CHECK(f.glyph(f.index(' ')).slot < 0, "a space has no atlas rect");
    f.warm(0x20, 0x24f);
    auto ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
    printf("info: %zu DejaVu Sans glyphs on %zu page(s) in %.1f ms\n", f.glyph_count(), f.page_count(), ms);
    std::vector<hud::drawlist::FaceGlyph> all;
    for (uint32_t cp = 0x21; cp <= 0x24f; cp++)
        if (uint32_t g = f.index(cp))
            if (hud::drawlist::FaceGlyph fg = f.glyph(g); fg.slot >= 0)
                all.push_back(fg);
    bool apart = true, inside = true;
    for (size_t i = 0; i < all.size(); i++) {
        inside = inside && all[i].u0 >= 0 && all[i].v0 >= 0 && all[i].u1 <= 1 && all[i].v1 <= 1;
        for (size_t j = i + 1; j < all.size() && apart; j++)
            apart = !overlaps(all[i], all[j]) || (all[i].u0 == all[j].u0 && all[i].v0 == all[j].v0);
    }
    CHECK(all.size() > 400 && f.page_count() >= 2, "Latin Extended fills more than one page");
    CHECK(apart && inside, "packed glyph rects stay inside their page and never overlap");
    CHECK(fs.reg.image(bar.slot)->generation > gen, "adding glyphs to a page bumps its generation for the upload");

    Fonts tight;
    for (int i = 0; i < 3; i++)
        tight.reg.set_image("img" + std::to_string(i), 1, 1, std::vector<uint8_t>(4, 255));
    std::string why;
    auto capped = TtfFace::open(read("../../apps/launcher/assets/fonts/DejaVuSans.ttf"), tight.reg, tight.lock, "p:", 2, why);
    CHECK(capped && capped->glyph(capped->index('A')).slot < 0, "a font draws nothing once the overlay has no image slot left");
}

static void test_cff(Fonts &fs)
{
    TtfFace &f = *fs.cff;
    hud::drawlist::FaceGlyph o = f.glyph(f.index('O'));
    const hud::drawlist::Image *page = fs.reg.image(o.slot);
    float cu = (o.u0 + o.u1) / 2, cv = (o.v0 + o.v1) / 2;
    float ring_u = o.u0 + (0.1f - o.x0) / (o.x1 - o.x0) * (o.u1 - o.u0);
    CHECK(page && texel(*page, cu, cv) < 60 && texel(*page, ring_u, cv) > 140, "a CFF O of cubic curves keeps its counter open");
    hud::drawlist::FaceGlyph a = f.glyph(f.index('A'));
    CHECK(near((a.y1 - a.y0) * 64, 0.7f * 64 + 16, 1.5f), "a CFF glyph box is its outline plus the spread");
    CHECK(near(f.advance(f.index('H')), 0.64f) && near(f.kern(f.index('A'), f.index('V')), -0.08f),
          "CFF advances and GPOS kerning come in em units");
    CHECK(near(f.baseline, (1 - 0.7f - 0.22f) / 2 + 0.7f), "the baseline centres the caps and descenders in the line box");
}

static void test_measure(Fonts &fs)
{
    hud::drawlist::TextStyle s;
    s.px = 32;
    s.face = fs.ttf.get();
    float a = 0, v = 0, av = 0, h = 0;
    hud::drawlist::measure(s, "A", &a, nullptr);
    hud::drawlist::measure(s, "V", &v, nullptr);
    hud::drawlist::measure(s, "AV", &av, &h);
    TtfFace &f = *fs.ttf;
    float k = f.kern(f.index('A'), f.index('V'));
    CHECK(k < 0 && near(av, a + v + k * 32), "measure applies the loaded font's kerning");
    CHECK(near(a, f.advance(f.index('A')) * 32) && h == 32, "a glyph is its advance wide and a line one size tall");
    float w12 = 0, w72 = 0, built = 0;
    s.px = 12;
    hud::drawlist::measure(s, "Lap 12 / 57", &w12, nullptr);
    s.px = 72;
    hud::drawlist::measure(s, "Lap 12 / 57", &w72, nullptr);
    hud::drawlist::measure(72, "Lap 12 / 57", &built, nullptr);
    CHECK(near(w72, w12 * 6, 1e-2f) && !near(w72, built, 1), "width scales with the size and differs from the built-in font");
    s.px = 20;
    s.tracking = 3;
    float tracked = 0, plain = 0;
    hud::drawlist::measure(s, "abc", &tracked, nullptr);
    s.tracking = 0;
    hud::drawlist::measure(s, "abc", &plain, nullptr);
    CHECK(near(tracked, plain + 6), "tracking adds pixels between glyphs of a loaded font");
    s.max_width = 60;
    std::vector<hud::drawlist::Line> lines;
    hud::drawlist::layout(s, "box box box box", lines);
    bool fits = lines.size() >= 2;
    for (auto &l : lines)
        fits = fits && l.width <= 60.001f;
    CHECK(fits, "max_width wraps a loaded font at spaces");
    s.max_width = 0;
    float w = 0;
    hud::drawlist::measure(s, "one\ntwo", &w, &h);
    CHECK(near(h, 20 + 24), "lines advance by line_height whatever the font");
}

static void test_fallback(Fonts &fs)
{
    hud::drawlist::TextStyle s;
    s.px = 40;
    s.face = fs.cff.get();
    CHECK(hud::drawlist::pick(s, 'A').face == fs.cff.get(), "a codepoint the font has comes from the font");
    CHECK(!hud::drawlist::pick(s, 'Z').face && hud::drawlist::pick(s, 'Z').builtin == hud::font::find('Z'),
          "a codepoint the font lacks comes from the built-in font");
    CHECK(hud::drawlist::pick(s, 0x4e2d).builtin == hud::font::find('?'), "a codepoint neither font has draws as the built-in ?");
    CHECK(hud::drawlist::pick(s, '\t').face == fs.cff.get() && hud::drawlist::pick(s, 0xa0).face == fs.cff.get(),
          "a tab and a no-break space draw as the font's space");
    float az = 0;
    hud::drawlist::measure(s, "AZ", &az, nullptr);
    CHECK(near(az, (fs.cff->advance(fs.cff->index('A')) + hud::font::find('Z')->advance) * 40), "no kerning across fonts");
    hud::drawlist::Frame f;
    hud::drawlist::begin(f, 1920, 1080);
    hud::drawlist::text(f, 100, 100, s,
                        "AZ\xe4\xb8\xad"
                        "V");
    int page_tex = fs.cff->glyph(fs.cff->index('A')).slot + 1;
    CHECK(f.verts.size() == 4 * 6 && f.batches.size() == 3 && f.batches[0].tex == page_tex &&
              f.batches[1].tex == hud::drawlist::TEX_FONT && f.batches[2].tex == page_tex,
          "a line mixing fonts draws in batches of its fonts' atlases");
}

static void test_draw(Fonts &fs)
{
    hud::drawlist::TextStyle s;
    s.px = 48;
    s.face = fs.cff.get();
    hud::drawlist::Frame f;
    hud::drawlist::begin(f, 1920, 1080);
    hud::drawlist::text(f, 0, 0, s, "HAVO");
    CHECK(f.verts.size() == 24 && f.verts[0].mode == hud::drawlist::sdf_mode(0, 0), "a loaded font draws one SDF quad per glyph");
    size_t at = f.verts.size();
    hud::drawlist::TextStyle plain = s;
    plain.face = nullptr;
    hud::drawlist::text(f, 0, 0, plain, "HAVO");
    CHECK(f.text_cache.map.size() == 2, "the layout cache keeps a string apart per font");
    CHECK(f.verts[at].x != f.verts[0].x || f.verts[at].u != f.verts[0].u,
          "the built-in font lays the same string out its own way");
    s.outline_rgba = 0x000000ff;
    s.outline_px = 3;
    at = f.verts.size();
    hud::drawlist::text(f, 0, 0, s, "H");
    uint32_t dilate = f.verts[at].mode >> 8 & 0xff;
    uint32_t want = uint32_t(std::clamp(3.0f / (2 * hud::ttf::SPREAD * 48 / hud::ttf::ATLAS_EM), 0.0f, 0.5f) * 510 + 0.5f);
    CHECK(f.verts.size() - at == 12 && dilate == want, "an outline dilates the loaded font's field by its own atlas scale");
    at = f.verts.size();
    s.outline_rgba = 0;
    s.align = SWOFTY_TEXT_LEFT | SWOFTY_TEXT_BASELINE;
    hud::drawlist::text(f, 0, 500, s, "H");
    float bottom = std::max({f.verts[at].y, f.verts[at + 1].y, f.verts[at + 2].y, f.verts[at + 4].y});
    CHECK(near(bottom, 500 + 8 * 48 / hud::ttf::ATLAS_EM, 1.01f), "baseline alignment puts the loaded font's baseline on y");
}

static float area(const std::vector<Pt> &p) { return hud::drawlist::signed_area(p); }

static bool point_in(const std::vector<Pt> &p, float x, float y)
{
    bool in = false;
    for (size_t i = 0, j = p.size() - 1; i < p.size(); j = i++)
        if ((p[i].y > y) != (p[j].y > y) && x < (p[j].x - p[i].x) * (y - p[i].y) / (p[j].y - p[i].y) + p[i].x)
            in = !in;
    return in;
}

static std::vector<Pt> star(float cx, float cy, float outer, float inner, int points, bool clockwise)
{
    std::vector<Pt> p;
    for (int i = 0; i < 2 * points; i++) {
        float a = float(i) * hud::drawlist::PI / float(points) * (clockwise ? 1.0f : -1.0f) - hud::drawlist::PI / 2;
        float r = i % 2 ? inner : outer;
        p.push_back({cx + r * std::cos(a), cy + r * std::sin(a)});
    }
    return p;
}

struct Tri {
    size_t count;
    float sum;
    bool oriented, inside;
};

static Tri check_tris(const std::vector<Pt> &p)
{
    std::vector<uint32_t> t;
    hud::drawlist::triangulate(p, t);
    Tri r{t.size() / 3, 0, true, true};
    float sign = area(p) > 0 ? 1.0f : -1.0f;
    for (size_t i = 0; i + 2 < t.size(); i += 3) {
        float a = hud::drawlist::turn(p[t[i]], p[t[i + 1]], p[t[i + 2]]) * 0.5f * sign;
        r.sum += a;
        r.oriented = r.oriented && a > 0;
        float cx = (p[t[i]].x + p[t[i + 1]].x + p[t[i + 2]].x) / 3, cy = (p[t[i]].y + p[t[i + 1]].y + p[t[i + 2]].y) / 3;
        r.inside = r.inside && point_in(p, cx, cy);
    }
    return r;
}

static void test_triangulate()
{
    for (bool cw : {false, true}) {
        std::vector<Pt> s = star(200, 200, 100, 40, 5, cw);
        Tri r = check_tris(s);
        CHECK(r.count == 8 && near(r.sum, std::fabs(area(s)), 1e-1f) && r.oriented && r.inside,
              cw ? "a clockwise five-point star is 8 triangles covering its area"
                 : "a counter-clockwise star is 8 triangles covering its area");
    }
    std::vector<Pt> l = {{0, 0}, {100, 0}, {100, 30}, {30, 30}, {30, 100}, {0, 100}};
    Tri rl = check_tris(l);
    CHECK(rl.count == 4 && near(rl.sum, 100 * 30 + 30 * 70) && rl.oriented && rl.inside, "an L shape triangulates inside itself");
    std::vector<Pt> comb = {{0, 0},    {100, 0},  {100, 100}, {80, 100}, {80, 20},  {60, 20},
                            {60, 100}, {40, 100}, {40, 20},   {20, 20},  {20, 100}, {0, 100}};
    Tri rc = check_tris(comb);
    CHECK(rc.count <= 10 && near(rc.sum, std::fabs(area(comb)), 1e-2f) && rc.oriented && rc.inside,
          "a comb with three teeth triangulates inside itself");
    std::reverse(comb.begin(), comb.end());
    Tri rr = check_tris(comb);
    CHECK(rr.count <= 10 && near(rr.sum, std::fabs(area(comb)), 1e-2f) && rr.oriented && rr.inside,
          "the comb wound the other way triangulates the same");
    std::vector<Pt> mids = {{0, 0}, {50, 0}, {100, 0}, {100, 50}, {100, 100}, {50, 100}, {0, 100}, {0, 50}};
    Tri rm = check_tris(mids);
    CHECK(rm.count <= 6 && near(rm.sum, 10000) && rm.oriented && rm.inside, "collinear points add no zero-area triangles");
    std::vector<Pt> u = {{0, 0}, {30, 0}, {30, 70}, {70, 70}, {70, 0}, {100, 0}, {100, 100}, {50, 100}, {0, 100}};
    Tri ru = check_tris(u);
    CHECK(ru.count <= 7 && near(ru.sum, std::fabs(area(u))) && ru.oriented && ru.inside,
          "a U with a collinear point on its base triangulates inside itself");
    std::vector<Pt> line = {{0, 0}, {50, 0}, {100, 0}};
    CHECK(check_tris(line).count == 0, "a polygon of collinear points has no triangles");
    std::vector<Pt> big = star(500, 500, 400, 150, 60, false);
    Tri rb = check_tris(big);
    CHECK(rb.count == big.size() - 2 && near(rb.sum, std::fabs(area(big)), 2) && rb.oriented && rb.inside,
          "a 60-point star triangulates inside itself");

    hud::drawlist::Frame f;
    hud::drawlist::begin(f, 1920, 1080);
    std::vector<Pt> s = star(300, 300, 100, 40, 5, true);
    std::vector<float> xy;
    for (Pt p : s)
        xy.insert(xy.end(), {p.x, p.y});
    xy.insert(xy.end(), {s[0].x, s[0].y});
    hud::drawlist::polygon_concave(f, xy.data(), uint32_t(xy.size() / 2), 0xffffffff);
    CHECK(f.verts.size() == 8 * 3 + 6 * 10, "a concave fill is its triangles plus a fringe of 6 vertices per point");
    float top = 1e9f;
    for (auto &v : f.verts)
        top = std::min(top, v.y);
    CHECK(top < 200 - 0.5f && top > 200 - 3, "the fringe reaches outside the star's points");
    size_t at = f.verts.size();
    hud::drawlist::push_opacity(f, 0.5f);
    hud::drawlist::polygon_concave(f, xy.data(), 3, 0xffffffff);
    hud::drawlist::pop_opacity(f);
    CHECK(f.verts.size() - at == 3 + 18 && (f.verts[at].rgba >> 24) == 128, "a concave fill takes the opacity stack");
}

static void test_flatten()
{
    using swofty::overlay::PathVertex;
    std::vector<float> xy;
    PathVertex square[4] = {{0, 0}, {100, 0}, {100, 100}, {0, 100}};
    CHECK(swofty::overlay::flatten_path(square, 4, true, xy) == 4, "a closed path of straight segments is its vertices");
    CHECK(swofty::overlay::flatten_path(square, 4, false, xy) == 4 && xy[6] == 0 && xy[7] == 100,
          "an open path ends at its last vertex");
    const float r = 100, k = 0.5523f * r;
    PathVertex circle[4] = {{r, 0, 0, -k, 0, k}, {0, r, k, 0, -k, 0}, {-r, 0, 0, k, 0, -k}, {0, -r, -k, 0, k, 0}};
    uint32_t n = swofty::overlay::flatten_path(circle, 4, true, xy, 0.25f);
    float worst = 0;
    for (uint32_t i = 0; i < n; i++)
        worst = std::max(worst, std::fabs(std::hypot(xy[2 * i], xy[2 * i + 1]) - r));
    CHECK(n >= 16 && worst < 0.3f && !(xy[2 * n - 2] == xy[0] && xy[2 * n - 1] == xy[1]),
          "a bezier circle flattens within the tolerance and does not repeat its first point");
    std::vector<float> fine;
    CHECK(swofty::overlay::flatten_path(circle, 4, true, fine, 0.01f) > n, "a smaller tolerance gives more points");
    CHECK(swofty::overlay::flatten_path(circle, 0, true, xy) == 0 && xy.empty(), "an empty path has no points");
}

static void raster(const hud::drawlist::Frame &f, const std::map<int, const hud::img::Pixels *> &tex, std::vector<float> &px,
                   int W, int H)
{
    for (const hud::drawlist::Batch &b : f.batches) {
        auto it = tex.find(b.tex);
        const hud::img::Pixels *t = it == tex.end() ? nullptr : it->second;
        auto sample = [&](float u, float v) {
            float x = u * float(t->width) - 0.5f, y = v * float(t->height) - 0.5f;
            int x0 = int(std::floor(x)), y0 = int(std::floor(y));
            float fx = x - float(x0), fy = y - float(y0);
            auto at = [&](int xx, int yy) {
                xx = std::clamp(xx, 0, int(t->width) - 1), yy = std::clamp(yy, 0, int(t->height) - 1);
                return float(t->rgba[size_t(yy) * t->width + size_t(xx)]) / 255.0f;
            };
            return (at(x0, y0) * (1 - fx) + at(x0 + 1, y0) * fx) * (1 - fy) +
                   (at(x0, y0 + 1) * (1 - fx) + at(x0 + 1, y0 + 1) * fx) * fy;
        };
        for (uint32_t i = b.first; i + 2 < b.first + b.count; i += 3) {
            const hud::drawlist::Vertex &a = f.verts[i], &bb = f.verts[i + 1], &c = f.verts[i + 2];
            float ar = (bb.x - a.x) * (c.y - a.y) - (bb.y - a.y) * (c.x - a.x);
            if (std::fabs(ar) < 1e-8f)
                continue;
            float dudx = ((bb.u - a.u) * (c.y - a.y) - (c.u - a.u) * (bb.y - a.y)) / ar;
            float dudy = (-(bb.u - a.u) * (c.x - a.x) + (c.u - a.u) * (bb.x - a.x)) / ar;
            int x0 = std::max(0, int(std::floor(std::min({a.x, bb.x, c.x})))),
                x1 = std::min(W, int(std::ceil(std::max({a.x, bb.x, c.x}))));
            int y0 = std::max(0, int(std::floor(std::min({a.y, bb.y, c.y})))),
                y1 = std::min(H, int(std::ceil(std::max({a.y, bb.y, c.y}))));
            for (int y = y0; y < y1; y++)
                for (int x = x0; x < x1; x++) {
                    float sx = float(x) + 0.5f, sy = float(y) + 0.5f;
                    float w1 = ((sx - a.x) * (c.y - a.y) - (sy - a.y) * (c.x - a.x)) / ar;
                    float w2 = ((bb.x - a.x) * (sy - a.y) - (bb.y - a.y) * (sx - a.x)) / ar;
                    float w0 = 1 - w1 - w2;
                    if (w0 < -1e-6f || w1 < -1e-6f || w2 < -1e-6f)
                        continue;
                    float col[4];
                    for (int k = 0; k < 4; k++)
                        col[k] = (float((a.rgba >> (8 * k)) & 0xff) * w0 + float((bb.rgba >> (8 * k)) & 0xff) * w1 +
                                  float((c.rgba >> (8 * k)) & 0xff) * w2) /
                                 255.0f;
                    float al = 1;
                    if ((a.mode & 0xff) == hud::drawlist::SDF && t) {
                        float u = a.u * w0 + bb.u * w1 + c.u * w2, v = a.v * w0 + bb.v * w1 + c.v * w2;
                        float d = sample(u, v);
                        float fw = std::max((std::fabs(dudx) + std::fabs(dudy)) * float(t->width) / 8.0f, 1e-5f);
                        float e = 0.5f - float((a.mode >> 8) & 255) / 510.0f,
                              h = fw * (0.5f + float((a.mode >> 16) & 255) / 16.0f);
                        float tt = std::clamp((d - (e - h)) / (2 * h), 0.0f, 1.0f);
                        al = tt * tt * (3 - 2 * tt);
                    }
                    float sa = al * col[3];
                    float *dst = &px[(size_t(y) * size_t(W) + size_t(x)) * 3];
                    for (int k = 0; k < 3; k++)
                        dst[k] = al * col[k] * col[3] + dst[k] * (1 - sa);
                }
        }
    }
}

static void test_scene(Fonts &fs, const char *out_dir)
{
    hud::drawlist::Frame f;
    hud::drawlist::begin(f, 1280, 720);
    hud::drawlist::TextStyle s;
    s.face = fs.ttf.get();
    float y = 20;
    for (float size : {14.0f, 24.0f, 40.0f, 72.0f}) {
        s.px = size;
        hud::drawlist::text(f, 20, y, s, "AVATAR Zürich 1:23.456 \xe4\xb8\xad");
        y += size * 1.25f;
    }
    hud::drawlist::TextStyle c;
    c.face = fs.cff.get();
    c.px = 120;
    c.outline_rgba = 0xe10600ff;
    c.outline_px = 4;
    c.shadow_rgba = 0x000000c0;
    c.shadow_dx = c.shadow_dy = 6;
    c.shadow_blur = 4;
    hud::drawlist::text(f, 20, 330, c, "HAVO g?Z");
    std::vector<Pt> st = star(1050, 200, 150, 60, 5, false);
    std::vector<float> xy;
    for (Pt p : st)
        xy.insert(xy.end(), {p.x, p.y});
    hud::drawlist::polygon_concave(f, xy.data(), uint32_t(xy.size() / 2), 0xffd700ff);
    using swofty::overlay::PathVertex;
    PathVertex blob[5] = {{900, 500, 0, 0, 80, -60},
                          {1150, 480, -60, -40, 40, 60},
                          {1100, 650, 50, 0, -40, 0},
                          {1000, 560, 30, 30, -30, -30},
                          {920, 680, 40, 0, -60, 0}};
    std::vector<float> pts;
    uint32_t n = swofty::overlay::flatten_path(blob, 5, true, pts);
    hud::drawlist::polygon_concave(f, pts.data(), n, 0x00d2beff);
    hud::drawlist::polyline(f, pts.data(), n, 3, 0xffffffff, SWOFTY_POLYLINE_CLOSED);
    CHECK(!f.dropped && f.verts.size() > 1000, "the fonts scene tessellates");
    if (!out_dir)
        return;
    hud::img::Pixels builtin;
    hud::img::grey(hud::font::ATLAS_PNG, hud::font::ATLAS_PNG_SIZE, builtin);
    std::vector<hud::img::Pixels> pages(fs.reg.image_count());
    std::map<int, const hud::img::Pixels *> tex{{hud::drawlist::TEX_FONT, &builtin}};
    for (size_t i = 0; i < pages.size(); i++)
        if (const hud::drawlist::Image *im = fs.reg.image(int(i)); im && im->channels == 1) {
            pages[i].width = im->width;
            pages[i].height = im->height;
            pages[i].rgba = im->rgba;
            tex[int(i) + 1] = &pages[i];
        }
    const int W = 1280, H = 720;
    std::vector<float> px(size_t(W) * H * 3, 0.3f);
    raster(f, tex, px, W, H);
    std::vector<uint8_t> rgb(px.size());
    for (size_t i = 0; i < px.size(); i++)
        rgb[i] = uint8_t(std::clamp(px[i], 0.0f, 1.0f) * 255 + 0.5f);
    std::string png = hud::png::encode_rgb(rgb.data(), W, H), path = std::string(out_dir) + "/fonts-scene.png";
    if (FILE *fp = fopen(path.c_str(), "wb")) {
        fwrite(png.data(), 1, png.size(), fp);
        fclose(fp);
        printf("wrote %s\n", path.c_str());
    }
}

int main(int argc, char **argv)
{
    if (argc < 3) {
        fprintf(stderr, "usage: overlay_fonts_test FONT.ttf FONT_CFF.otf [OUT_DIR]\n");
        return 2;
    }
    std::string ttf = read(argv[1]), cff = read(argv[2]);
    CHECK(!ttf.empty() && !cff.empty(), "the test fonts read");
    Fonts fs;
    test_loader(fs, ttf, cff);
    if (!fs.ttf || !fs.cff) {
        printf("\n%d check(s) failed\n", failures + 1);
        return 1;
    }
    test_atlas(fs);
    test_cff(fs);
    test_measure(fs);
    test_fallback(fs);
    test_draw(fs);
    test_triangulate();
    test_flatten();
    test_scene(fs, argc > 3 ? argv[3] : nullptr);
    printf(failures ? "\n%d check(s) failed\n" : "\nall overlay font checks passed\n", failures);
    return failures ? 1 : 0;
}
