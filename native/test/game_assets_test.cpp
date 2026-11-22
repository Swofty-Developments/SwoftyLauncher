#include "core/assets/game_files.hpp"

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>

namespace gf = swofty_core::assets::game_files;
namespace erp = swofty_core::apply::content::erpfile;
namespace fs = std::filesystem;

static int failures = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failures++;
}

template <class T> static void put(std::string &s, T v) { s.append(reinterpret_cast<const char *>(&v), sizeof v); }

struct Res {
    std::string name;
    std::vector<std::string> frags;
};

static std::string make_erp(const std::vector<Res> &list)
{
    std::string index, data;
    put(index, uint32_t(list.size()));
    put(index, uint32_t(list.size()));
    for (auto &r : list) {
        std::string entry(4, '\0');
        put(entry, uint16_t(r.name.size() + 1));
        entry += r.name;
        entry += '\0';
        entry.append(20, '\0');
        entry += char(r.frags.size());
        for (auto &f : r.frags) {
            entry += "temp";
            put(entry, uint64_t(data.size()));
            put(entry, uint64_t(f.size()));
            put(entry, uint32_t(16));
            entry += '\x10';
            put(entry, uint64_t(f.size()));
            data += f;
        }
        entry.append(16, '\0');
        uint32_t length = uint32_t(entry.size() - 4);
        std::memcpy(entry.data(), &length, 4);
        index += entry;
    }
    std::string bytes = "ERPK";
    put(bytes, uint32_t(3));
    bytes.append(8, '\0');
    put(bytes, uint64_t(48));
    put(bytes, uint64_t(index.size()));
    put(bytes, uint64_t(48 + index.size()));
    bytes.append(8, '\0');
    return bytes + index + data;
}

static gf::Env disk_env(const fs::path &root)
{
    gf::Env env;
    env.list = [root](const std::string &folder) {
        std::vector<std::string> out;
        std::error_code ec;
        for (fs::recursive_directory_iterator it(root / folder, ec), end; !ec && it != end; it.increment(ec))
            if (it->is_regular_file(ec) && it->path().extension() == ".erp")
                out.push_back(it->path().lexically_relative(root).generic_string());
        std::sort(out.begin(), out.end());
        return out;
    };
    env.open = [root](const std::string &rel) -> std::shared_ptr<erp::Source> {
        std::shared_ptr<std::FILE> f(std::fopen((root / rel).string().c_str(), "rb"), [](std::FILE *p) {
            if (p)
                std::fclose(p);
        });
        if (!f)
            return nullptr;
        auto src = std::make_shared<erp::Source>();
        std::error_code ec;
        src->size = fs::file_size(root / rel, ec);
        src->read = [f](uint64_t off, size_t n, char *out) {
            return std::fseek(f.get(), long(off), SEEK_SET) == 0 && std::fread(out, 1, n, f.get()) == n;
        };
        return src;
    };
    return env;
}

static void write(const fs::path &p, const std::string &bytes)
{
    fs::create_directories(p.parent_path());
    std::ofstream(p, std::ios::binary) << bytes;
}

static void synthetic(const fs::path &dir)
{
    fs::remove_all(dir);
    std::string head(32, '\0');
    uint32_t fields[] = {52, 8, 4, 0, 1, 2};
    std::memcpy(&head[8], &fields[0], 12);
    std::memcpy(&head[24], &fields[4], 8);
    std::string red = std::string("\x00\xf8\x00\xf8", 4) + std::string(4, '\0');
    std::string blue = std::string("\x1f\x00\x1f\x00", 4) + std::string(4, '\0');
    std::string pixels = red + blue + blue + red;
    write(dir / "2025_asset_groups/test_package/a.erp",
          make_erp({{"eaid://test_package/tex.image", {head, pixels}}, {"eaid://test_package/two.bin", {"abc", "defg"}}}));
    write(dir / "2025_asset_groups/test_package/sub/b.erp", make_erp({{"eaid://test_package/only_b.bin", {"bee"}}}));
    write(dir / "swofty/over.erp", make_erp({{"eaid://test_package/two.bin", {"over"}}}));

    gf::Env env = disk_env(dir);
    env.overlays = [](const std::string &rel) {
        return rel == "2025_asset_groups/test_package/a.erp" ? std::vector<std::string>{"swofty/over.erp"}
                                                             : std::vector<std::string>{};
    };
    gf::Files files(env);
    std::string out, why;
    expect(files.read("eaid://test_package/only_b.bin", ~0u, out, why) && out == "bee",
           "a resource in a subfolder's archive reads");
    expect(files.read("eaid://test_package/two.bin", ~0u, out, why) && out == "over",
           "an overlay archive wins over the game archive");
    expect(!files.read("eaid://test_package/missing.bin", ~0u, out, why) && why.find("is in no archive") != std::string::npos,
           "a URI no archive carries is refused with a reason");
    expect(!files.read("eaid://test_package/only_b.bin", 3, out, why), "a fragment past the last is refused");
    expect(!files.read("file://x", ~0u, out, why) && !files.read("eaid://", ~0u, out, why),
           "a URI that is not eaid:// is refused");
    gf::Texture t;
    std::vector<uint8_t> rgba;
    expect(files.rgba("eaid://test_package/tex.image", 1, 0, t, rgba, why) && t.format == 52 && t.width == 8 && t.height == 4 &&
               t.layers == 2 && rgba.size() == 8 * 4 * 4,
           "a BC1 array layer decodes to RGBA8");
    expect(rgba.size() == 128 && rgba[0] < 8 && rgba[2] > 240 && rgba[3] == 255 && rgba[4 * 4] > 240 && rgba[4 * 4 + 2] < 8,
           "layer 1 is blue then red");
    expect(!files.rgba("eaid://test_package/tex.image", 2, 0, t, rgba, why), "a layer past the last is refused");
    expect(files.archives_opened() == 3, "each archive's index opens once");
    fs::remove_all(dir);
}

static void real(const fs::path &game)
{
    if (!fs::exists(game / "2025_asset_groups/ui_package/databases.erp")) {
        std::printf("skip real game files: %s has no ui_package\n", game.string().c_str());
        return;
    }
    gf::Env env = disk_env(game);
    env.indexed = [](const std::string &folder, size_t archives, size_t uris) {
        std::printf("     %s: %zu archive(s), %zu resource(s) indexed\n", folder.c_str(), archives, uris);
    };
    gf::Files files(env);
    auto started = std::chrono::steady_clock::now();
    std::string font, why;
    bool ok = files.read("eaid://ui_package/art/fonts/formula1-tv-600_bold_web.font", ~0u, font, why);
    double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count();
    std::printf("     ui_package: %zu archive index(es) opened, first read %.0f ms\n", files.archives_opened(), ms);
    expect(ok && font.size() > 10000, ("the F1 TV font reads from the game files: " + why).c_str());
    expect(font.find(std::string("\x00\x01\x00\x00", 4)) != std::string::npos, "it carries a TrueType font");
    gf::Catalogue c;
    ok = files.catalogue("overrides.team_logos.driver_career_2024_team_select", c, why);
    expect(ok && c.names.size() >= 60 && c.texture.starts_with("eaid://ui_package/art/texture_arrays/"),
           ("the team logo catalogue lists its names and texture array: " + why).c_str());
    auto it = std::find(c.names.begin(), c.names.end(), "redbull");
    expect(it != c.names.end(), "the catalogue has redbull");
    gf::Texture t;
    std::vector<uint8_t> rgba;
    ok = it != c.names.end() && files.rgba(c.texture, uint32_t(it - c.names.begin()), 0, t, rgba, why);
    size_t opaque = 0;
    for (size_t i = 3; i < rgba.size(); i += 4)
        opaque += rgba[i] > 8;
    std::printf("     %s: format %u, %ux%u, %u level(s), %u layer(s); redbull %zu opaque pixel(s)\n", c.texture.c_str(), t.format,
                t.width, t.height, t.levels, t.layers, opaque);
    expect(ok && t.layers >= c.names.size() && opaque > 1000, ("the redbull layer decodes with an opaque logo: " + why).c_str());
    expect(!files.catalogue("overrides.no_such.list", c, why), "a catalogue the database lacks is refused");
}

int main()
{
    synthetic(fs::temp_directory_path() / ("game_assets_test." + std::to_string(std::rand())));
    const char *dir = std::getenv("F1_25_DIR");
    if (dir && *dir)
        real(dir);
    else
        std::printf("skip real game files: F1_25_DIR is not set\n");
    std::printf("%s\n", failures ? "game_assets_test: FAILED" : "game_assets_test: all passed");
    return failures ? 1 : 0;
}
