#define ZSTD_STATIC_LINKING_ONLY
#include "../../src/core/apply/content/logo_build.hpp"
#include "../../src/core/apply/content/png_straight.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <set>
#include <sstream>

namespace content = swofty_core::apply::content;
namespace lb = content::logo_build;
namespace ef = content::erpfile;
namespace fs = std::filesystem;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

static std::string slurp(const fs::path &path)
{
    std::ifstream f(path, std::ios::binary);
    std::stringstream s;
    s << f.rdbuf();
    return s.str();
}

static std::shared_ptr<ef::Source> open_file(const fs::path &path)
{
    if (!fs::exists(path))
        return nullptr;
    auto in = std::make_shared<std::ifstream>(path, std::ios::binary);
    auto src = std::make_shared<ef::Source>();
    src->size = fs::file_size(path);
    src->read = [in](uint64_t off, size_t n, char *out) {
        in->clear();
        in->seekg(std::streamoff(off));
        in->read(out, std::streamsize(n));
        return size_t(in->gcount()) == n;
    };
    return src;
}

static std::string rewrite_to_string(const ef::Source &src, const ef::Archive &arc, const ef::Changes &changes, std::string &why)
{
    std::string out;
    ef::rewrite(src, arc, changes, [&](const char *p, size_t n) { out.append(p, n); return true; }, why);
    return out;
}

int main(int argc, char **argv)
{
    fs::path dir = argc > 1 ? argv[1] : "content/bc7";
    fs::path game = dir / "game";
    std::string why;

    std::string raw = slurp(dir / "shape64.rgba"), png_bytes = slurp(dir / "shape64.png"), decoded;
    const char *px = raw.data() + raw.find('\n') + 1;
    uint32_t w = 0, h = 0;
    check(content::png::decode(png_bytes, decoded, w, h, why) && w == 64 && h == 64 && decoded == std::string(px, 64 * 64 * 4),
          "a PNG decodes to straight alpha with zero colour under alpha 0");

    std::vector<std::string> list = {"2025_asset_groups/ui_package/databases.erp", "2025_asset_groups/ui_package/flow_a.erp",
                                     "2025_asset_groups/ui_package/flow_none.erp", "2025_asset_groups/ui_package/screens/b.erp"};
    for (auto &rel : list) {
        auto src = open_file(game / rel);
        ef::Archive arc;
        bool ok = src && ef::open(*src, arc, why);
        check(ok && rewrite_to_string(*src, arc, {}, why) == slurp(game / rel), rel + ": rewriting with no change gives the same bytes");
    }

    {
        uint64_t seed = 7;
        auto noisy = [&](size_t n) {
            std::string out(n, '\0');
            for (size_t i = 0; i < n; i++) {
                seed = seed * 6364136223846793005ull + 1442695040888963407ull;
                out[i] = i % 4 ? "livery_"[i % 7] : char(seed >> 56);
            }
            return out;
        };
        for (size_t n : {size_t(0), size_t(300), size_t(70) << 10, size_t(1) << 20, (size_t(3) << 20) + 5}) {
            for (int level : {3, ef::ZSTD_LEVEL, 19}) {
                std::string data = noisy(n), z, back(n, '\0');
                ZSTD_FrameHeader h{};
                bool ok = swofty_core::erp_zstd::compress(data.data(), n, level, z) && ZSTD_getFrameHeader(&h, z.data(), z.size()) == 0;
                size_t buffer = ok ? ZSTD_decodingBufferSize_min(h.windowSize, h.frameContentSize) + std::max<size_t>(h.blockSizeMax, 4) : 0;
                ok = ok && h.frameContentSize == n && h.windowSize <= (1u << 20) && buffer <= 1441856 &&
                     ZSTD_decompress(back.data(), n, z.data(), z.size()) == n && back == data;
                check(ok, std::to_string(n) + " bytes at level " + std::to_string(level) + ": the frame names its size and a window of " +
                              std::to_string(h.windowSize) + " bytes, a " + std::to_string(buffer) + "-byte decoder buffer");
            }
        }
    }

    std::string uidb = slurp(dir / "expect/overrides.uidb");
    content::uidb::Pkg pkg;
    check(pkg.read(uidb) && pkg.write() == uidb, "overrides.uidb reads and writes to the same bytes");

    {
        namespace la = content::logo_array;
        auto px_of = [](const std::string &img, uint32_t w, uint32_t x, uint32_t y) {
            return reinterpret_cast<const uint8_t *>(img.data() + (size_t(y) * w + x) * 4);
        };
        std::string solid(8 * 8 * 4, '\0');
        for (size_t i = 0; i < 64; i++) {
            solid[i * 4] = char(200);
            solid[i * 4 + 1] = 100;
            solid[i * 4 + 2] = 50;
            solid[i * 4 + 3] = char(255);
        }
        std::string same = la::fit_rgba(reinterpret_cast<const uint8_t *>(solid.data()), 8, 8, 8, 8);
        check(same == solid, "fit at the exact size keeps every pixel");
        std::string small = la::fit_rgba(reinterpret_cast<const uint8_t *>(solid.data()), 8, 8, 4, 4);
        bool flat = true;
        for (uint32_t y = 0; y < 4; y++)
            for (uint32_t x = 0; x < 4; x++) {
                auto p = px_of(small, 4, x, y);
                flat &= p[0] == 200 && p[1] == 100 && p[2] == 50 && p[3] == 255;
            }
        check(flat, "shrinking an opaque image keeps colour and alpha at the edges");
        std::string big = la::fit_rgba(reinterpret_cast<const uint8_t *>(solid.data()), 8, 8, 16, 16);
        flat = true;
        for (uint32_t y = 0; y < 16; y++)
            for (uint32_t x = 0; x < 16; x++) {
                auto p = px_of(big, 16, x, y);
                flat &= p[0] == 200 && p[1] == 100 && p[2] == 50 && p[3] == 255;
            }
        check(flat, "growing an opaque image keeps colour and alpha at the edges");
        std::string checker(2 * 2 * 4, '\0');
        for (int i = 0; i < 4; i++) {
            bool white = (i == 0 || i == 3);
            checker[i * 4] = checker[i * 4 + 1] = checker[i * 4 + 2] = white ? char(255) : 0;
            checker[i * 4 + 3] = char(255);
        }
        std::string avg = la::fit_rgba(reinterpret_cast<const uint8_t *>(checker.data()), 2, 2, 1, 1);
        check(px_of(avg, 1, 0, 0)[0] >= 127 && px_of(avg, 1, 0, 0)[0] <= 128 && px_of(avg, 1, 0, 0)[3] == 255,
              "shrinking averages the covered pixels (area average)");
        std::string wide(8 * 4 * 4, '\0');
        for (size_t i = 0; i < 32; i++) {
            wide[i * 4] = char(255);
            wide[i * 4 + 3] = char(255);
        }
        std::string fit = la::fit_rgba(reinterpret_cast<const uint8_t *>(wide.data()), 8, 4, 16, 16);
        bool top_clear = true, middle = true, bottom_clear = true;
        for (uint32_t x = 0; x < 16; x++) {
            for (uint32_t y = 0; y < 4; y++)
                top_clear &= px_of(fit, 16, x, y)[3] == 0;
            for (uint32_t y = 4; y < 12; y++)
                middle &= px_of(fit, 16, x, y)[3] == 255 && px_of(fit, 16, x, y)[0] == 255;
            for (uint32_t y = 12; y < 16; y++)
                bottom_clear &= px_of(fit, 16, x, y)[3] == 0;
        }
        check(top_clear && middle && bottom_clear, "a non-square image keeps its aspect, is centred and the padding is transparent");
        std::string half(4 * 4 * 4, '\0');
        for (uint32_t y = 0; y < 4; y++)
            for (uint32_t x = 2; x < 4; x++) {
                size_t i = (size_t(y) * 4 + x) * 4;
                half[i] = char(255);
                half[i + 3] = char(255);
            }
        std::string edge = la::fit_rgba(reinterpret_cast<const uint8_t *>(half.data()), 4, 4, 2, 2);
        check(px_of(edge, 2, 0, 0)[3] == 0 && px_of(edge, 2, 0, 0)[0] == 0 && px_of(edge, 2, 1, 0)[3] == 255 && px_of(edge, 2, 1, 0)[0] == 255,
              "transparent pixels do not bleed colour into opaque ones (straight alpha)");
    }

    lb::Env env;
    env.list = [&] { return list; };
    env.open = [&](const std::string &rel) { return open_file(game / rel); };
    std::vector<lb::Want> wants(2);
    wants[0] = {"teams.driver_select_team_logos", "newteam", "", std::string(px, 64 * 64 * 4), 64, 64};
    wants[1].array = "teams.driver_select_team_logos";
    wants[1].key = "newalias";
    wants[1].donor = "b";
    lb::Result res;
    check(lb::build(env, wants, res) && res.built == 2 && res.refused == 0, "two slices are built");
    check(res.changes.size() == 3 && res.changes.count(lb::DATABASES) && res.changes.count(list[1]) && res.changes.count(list[3]) &&
              !res.changes.count(list[2]),
          "the two carriers and databases.erp change, the archive without the array does not");
    check(res.read == std::set<std::string>{lb::DATABASES, list[1], list[3]}, "the build reads databases.erp and the two carriers");

    fs::path out = argc > 2 ? argv[2] : "../build/bc7/logo_out";
    fs::create_directories(out);
    for (auto &[rel, changes] : res.changes) {
        auto src = open_file(game / rel);
        ef::Archive arc;
        ef::open(*src, arc, why);
        std::string built = rewrite_to_string(*src, arc, changes, why);
        std::string name = rel.substr(rel.rfind('/') + 1);
        if (rel.find("screens/") != std::string::npos)
            name = "screens_" + name;
        std::ofstream(out / name, std::ios::binary) << built;
    }
    check(true, "archives written to " + out.string() + " for check_logo_build.py");

    {
        fs::path ov = out / "overlay";
        fs::create_directories(ov / "ov");
        std::vector<lb::Want> first(1);
        first[0] = {"teams.driver_select_team_logos", "newteam", "", std::string(px, 64 * 64 * 4), 64, 64};
        lb::Result r1;
        lb::build(env, first, r1);
        auto count_of = [&](const fs::path &file, const std::string &rel_image) -> uint32_t {
            auto src = open_file(file);
            ef::Archive arc;
            std::string head, w2;
            if (!src || !ef::open(*src, arc, w2))
                return 0;
            const ef::Entry *e = arc.find(rel_image);
            if (!e || e->frags.empty() || !ef::fragment(*src, arc, e->frags[0], head, w2))
                return 0;
            return content::logo_array::le32(head, content::logo_array::HEAD_COUNT);
        };
        std::string image = std::string(lb::ARRAY_URI) + "teams.driver_select_team_logos.texarray.image";
        for (auto &[rel, changes] : r1.changes) {
            auto src = open_file(game / rel);
            ef::Archive arc;
            ef::open(*src, arc, why);
            std::string name = "ov/" + std::to_string(std::hash<std::string>{}(rel)) + ".erp";
            std::ofstream(ov / name, std::ios::binary) << rewrite_to_string(*src, arc, changes, why);
        }
        auto overlay_of = [&](const std::string &rel) { return "ov/" + std::to_string(std::hash<std::string>{}(rel)) + ".erp"; };
        uint32_t stock_count = count_of(game / list[1], image);
        lb::Env layered;
        layered.list = [&] { return list; };
        layered.open = [&](const std::string &rel) { return rel.rfind("ov/", 0) == 0 ? open_file(ov / rel) : open_file(game / rel); };
        layered.overlays = [&](const std::string &rel) {
            std::vector<std::string> v;
            if (r1.changes.count(rel))
                v.push_back(overlay_of(rel));
            return v;
        };
        std::vector<lb::Want> second(1);
        second[0].array = "teams.driver_select_team_logos";
        second[0].key = "second";
        second[0].donor = "b";
        lb::Result r5;
        check(lb::build(layered, second, r5) && r5.built == 1 && r5.refused == 0, "a slice is added on top of an overlay that already grew the array");
        std::string ovrel = overlay_of(list[1]);
        bool both = r5.changes.count(ovrel) && r5.changes.count(list[1]);
        check(both && r5.read.count(ovrel), "the overlay archive and the stock archive both change and the overlay is read");
        if (both) {
            for (auto &[rel, changes] : r5.changes) {
                auto src = layered.open(rel);
                ef::Archive arc;
                ef::open(*src, arc, why);
                std::ofstream(ov / ("out_" + std::to_string(std::hash<std::string>{}(rel)) + ".erp"), std::ios::binary)
                    << rewrite_to_string(*src, arc, changes, why);
            }
            uint32_t over = count_of(ov / ("out_" + std::to_string(std::hash<std::string>{}(ovrel)) + ".erp"), image);
            uint32_t stock_out = count_of(ov / ("out_" + std::to_string(std::hash<std::string>{}(list[1])) + ".erp"), image);
            check(over == stock_count + 2, "the overlay's array keeps the slice it already had and gains the new one (" + std::to_string(over) + " of " + std::to_string(stock_count) + " + 2)");
            check(stock_out == over, "the stock archive carries the same grown array, so the game gets it from either");
        }
        lb::Result r6, r7;
        lb::build(env, second, r6);
        lb::Env nolayer = layered;
        nolayer.overlays = nullptr;
        nolayer.open = env.open;
        lb::build(nolayer, second, r7);
        bool same = r6.changes.size() == r7.changes.size();
        for (auto &[rel, changes] : r6.changes)
            same = same && r7.changes.count(rel) && r7.changes[rel] == changes;
        check(same, "with no overlay the changes are the ones built from the stock archives");
    }

    {
        auto bad = wants;
        bad[0].width = 32;
        lb::Result r2;
        lb::build(env, bad, r2);
        bool said = false;
        for (auto &l : r2.lines)
            said |= l == "team.logo: newteam scaled 32x64 -> 64x64 for teams.driver_select_team_logos";
        check(r2.refused == 0 && r2.built == 2 && said, "an image of another size is scaled to the slice and logged once");
        lb::Result r2x;
        lb::build(env, wants, r2x);
        check(r2x.built == 2 && std::none_of(r2x.lines.begin(), r2x.lines.end(), [](const std::string &l) { return l.find("scaled") != std::string::npos; }),
              "an image of the exact size is not scaled and nothing is logged");
        bad = wants;
        bad[1].donor = "zz";
        lb::Result r3;
        lb::build(env, bad, r3);
        check(r3.refused == 1, "a donor key that is not listed is refused");
        bad[1].fallback = true;
        lb::Result r3f;
        lb::build(env, bad, r3f);
        check(r3f.refused == 0, "a donor fallback whose key is not listed is left out without a refusal");
        bad = wants;
        bad[0].array = "teams.nothing";
        lb::Result r4;
        lb::build(env, bad, r4);
        check(r4.refused >= 1, "an array that is not in overrides.uidb is refused");
    }

    if (const char *g = std::getenv("F1_GAME")) {
        fs::path ui = fs::path(g) / "2025_asset_groups/ui_package";
        auto src = open_file(ui / "databases.erp");
        ef::Archive arc;
        check(src && ef::open(*src, arc, why) && rewrite_to_string(*src, arc, {}, why) == slurp(ui / "databases.erp"),
              "the game's databases.erp rewrites to the same bytes");
        const ef::Entry *e = arc.find(lb::OVERRIDES);
        std::string real;
        content::uidb::Pkg p;
        check(e && ef::fragment(*src, arc, e->frags[0], real, why) && p.read(real) && p.write() == real, "the game's overrides.uidb writes to the same bytes");
        std::vector<std::string> all;
        for (auto &it : fs::recursive_directory_iterator(ui))
            if (it.path().extension() == ".erp")
                all.push_back("2025_asset_groups/ui_package/" + fs::relative(it.path(), ui).generic_string());
        std::sort(all.begin(), all.end());
        lb::Env real_env;
        real_env.list = [&] { return all; };
        real_env.open = [&](const std::string &rel) { return open_file(fs::path(g) / rel); };
        std::vector<lb::Want> real_wants;
        for (auto *a : {"teams.driver_select_team_logos", "osd.timing_tower.team_badges_hud", "team_logos.driver_career_2024_team_select",
                        "teams.badges_broadcast"}) {
            lb::Want w;
            w.array = a;
            w.key = "w26_probe";
            w.donor = std::string(a) == "osd.timing_tower.team_badges_hud" ? "red_bull" : "williams";
            real_wants.push_back(w);
        }
        lb::Result rr;
        lb::build(real_env, real_wants, rr);
        for (auto &l : rr.lines)
            std::printf("      %s\n", l.c_str());
        check(rr.built == 4 && rr.refused == 0 && rr.changes.count(lb::DATABASES), "the game's own archives build four probe slices");
    }
    return failures ? 1 : 0;
}
