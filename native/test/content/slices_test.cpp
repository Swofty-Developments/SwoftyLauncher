#include "core/apply/content/slices.hpp"
#include "core/apply/content/svg_raster.hpp"

#include <cstdio>
#include <fstream>
#include <sstream>
#include <set>

using swofty::json::Value;
namespace slices = swofty_core::apply::content::slices;
namespace svg = swofty_core::apply::content::svg;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

static Value rec(const char *array, const char *name, const char *image)
{
    return Value::object({{"array", Value(array)}, {"name", Value(name)}, {"image", Value(image)}});
}

static bool any_line(const slices::Lowering &l, const std::string &part)
{
    for (auto &line : l.lines)
        if (line.find(part) != std::string::npos)
            return true;
    return false;
}

static slices::Lowering lower(const std::vector<std::pair<std::string, Value>> &records, const std::set<std::string> &files)
{
    std::vector<slices::Input> in;
    for (auto &[id, v] : records)
        in.push_back({id, &v});
    return slices::lower(in,
                         [&](const std::string &addon, const std::string &file) { return files.count(addon + "/" + file) != 0; });
}

int main(int argc, char **argv)
{
    const char *DS = "overrides.teams.driver_select_team_logos";
    std::set<std::string> files{"mod.a/a.png", "mod.a/b.png", "mod.b/c.png", "mod.b/d.svg", "mod.b/e.tga", "mod.c/f.png"};

    auto two = lower({{"mod.a/one", rec(DS, "cs17_toro_rosso", "a.png")}, {"mod.b/two", rec(DS, "f12_lotus", "c.png")}}, files);
    check(two.applied == 2 && two.refused == 0 && two.contested == 0 && two.images.size() == 2,
          "two addons add different names to one array: both slices lower, none contested");
    check(two.images[0].array == "teams.driver_select_team_logos" && two.images[0].key == "cs17_toro_rosso" &&
              two.images[0].file == "a.png" && two.images[0].addon == "mod.a" && two.images[0].context == "ui.texture_slice" &&
              two.images[1].key == "f12_lotus" && two.images[1].addon == "mod.b",
          "each slice is a logo image of the array under the record's name, from its addon's file");
    check(
        two.arrays.size() == 1 && two.arrays["teams.driver_select_team_logos"].slices == 2 &&
            any_line(two, "ui.texture_slice teams.driver_select_team_logos: 2 slices from 2 addons (mod.a, mod.b), 0 contested"),
        "the array line counts slices and addons");

    auto clash = lower({{"mod.a/one", rec(DS, "shared", "a.png")}, {"mod.b/two", rec(DS, "shared", "c.png")}}, files);
    check(clash.contested == 1 && clash.images.size() == 1 && clash.images[0].file == "c.png" &&
              clash.images[0].addon == "mod.b" &&
              any_line(clash, "teams.driver_select_team_logos/shared: contested between mod.a and mod.b; mod.b is later") &&
              any_line(clash, "1 slice from 2 addons (mod.a, mod.b), 1 contested"),
          "the same name from two addons is contested and the later addon wins");

    std::vector<std::pair<std::string, Value>> many;
    std::set<std::string> big = files;
    for (int i = 0; i < 34; i++) {
        std::string addon = i < 12 ? "mod.a" : i < 20 ? "mod.b" : "mod.c";
        std::string file = "s" + std::to_string(i) + ".png";
        big.insert(addon + "/" + file);
        many.push_back({addon + "/r" + std::to_string(i),
                        rec(i % 2 ? "teams.driver_select_team_logos" : DS, ("n" + std::to_string(i)).c_str(), file.c_str())});
    }
    auto three = lower(many, big);
    check(three.images.size() == 34 && three.contested == 0 &&
              any_line(
                  three,
                  "ui.texture_slice teams.driver_select_team_logos: 34 slices from 3 addons (mod.a, mod.b, mod.c), 0 contested"),
          "34 slices from three addons on one array all apply, with or without the overrides. prefix");

    auto spelled = lower({{"mod.a/one", rec("overrides.teams.driver_select_team_logo", "x", "a.png")},
                          {"mod.a/two", rec("teams.driver_select_team_logos", "y", "b.png")}},
                         files);
    check(spelled.refused == 0 && spelled.images.size() == 2 && spelled.images[0].array == spelled.images[1].array,
          "the singular spelling the docs used and the game's plural name are one array");

    auto home = lower({{"mod.a/era", rec("overrides.home_screen.tiles_solo_6x6", "cs_f1_2017", "a.png")}}, files);
    check(home.refused == 0 && home.images.size() == 1 && home.images[0].array == "home_screen.tiles_solo_6x6",
          "the home screen 6x6 tile array is a known array");

    auto unknown = lower({{"mod.a/bad", rec("overrides.teams.no_such_array", "x", "a.png")}}, files);
    check(unknown.refused == 1 && unknown.images.empty() &&
              any_line(unknown,
                       "ui.texture_slice mod.a/bad: not applied: array overrides.teams.no_such_array is not a texture array "
                       "the core grows (known: overrides.teams.driver_select_team_logos, "),
          "an unknown array is refused and the known names are listed");

    auto missing = lower({{"mod.a/gone", rec(DS, "x", "nowhere.png")}}, files);
    check(missing.refused == 1 && any_line(missing, "image nowhere.png is not in swofty/addons/mod.a/"),
          "a missing image is refused and names the folder");

    auto kinds = lower({{"mod.b/vec", rec(DS, "vec", "d.svg")}, {"mod.b/tga", rec(DS, "tga", "e.tga")}}, files);
    check(kinds.images.size() == 1 && kinds.images[0].file == "d.svg" && kinds.refused == 1 &&
              any_line(kinds, "image e.tga is not a .png or .svg file"),
          "an svg lowers like a png and another format is refused");

    auto names = lower(
        {{"mod.a/n1", rec(DS, "has space", "a.png")}, {"mod.a/n2", rec(DS, "", "a.png")}, {"mod.a/n3", rec(DS, "a[b]", "a.png")}},
        files);
    check(names.refused == 3 && names.images.empty(), "names with a space, a bracket or nothing are refused");

    std::string vector_art = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\">"
                             "<rect x=\"0\" y=\"0\" width=\"50\" height=\"50\" fill=\"#ff0000\"/></svg>";
    std::string rgba, why;
    uint32_t w = 0, h = 0;
    bool ok = svg::decode(vector_art, rgba, w, h, why);
    auto px = [&](uint32_t x, uint32_t y) { return rgba.data() + (size_t(y) * w + x) * 4; };
    check(ok && w == 1024 && h == 512 && rgba.size() == size_t(w) * h * 4, "an svg rasterises to 1024 on its long side");
    check(ok && (unsigned char)px(100, 256)[0] == 255 && (unsigned char)px(100, 256)[1] == 0 &&
              (unsigned char)px(100, 256)[3] == 255 && (unsigned char)px(800, 256)[3] == 0,
          "the filled shape is opaque red and the rest is transparent");
    std::string bad_rgba;
    check(!svg::decode("not svg at all", bad_rgba, w, h, why) && !why.empty() && !svg::decode("", bad_rgba, w, h, why),
          "text that is not an svg is refused with a reason");
    check(svg::is_svg("logo.SVG", "") && svg::is_svg("logo", "<?xml?><svg ") && !svg::is_svg("logo.png", "\x89PNG"),
          "svg is told apart by extension or content");

    auto cs = lower({{"mod.a/era", rec("overrides.home_screen.tiles_solo_6x6", "cs_f1_2017", "a.png")}}, files);
    const char *TILES = "#overrides.home_screen.tiles_solo_6x6";
    check(slices::missing_slice(std::string(TILES) + "[cs_f1_2017]", cs.provided, "f1_2017").empty(),
          "an era image naming a slice an addon provides has no warning");
    std::string gone = slices::missing_slice(std::string(TILES) + "[cs_typo]", cs.provided, "f1_2017");
    check(gone == "image slice cs_typo of home_screen.tiles_solo_6x6 is not provided by any enabled ui.texture_slice record "
                  "(a slice the game ships in that array still shows)",
          "an era image naming a slice nobody provides warns with the slice and array");
    check(slices::missing_slice(std::string(TILES) + "[f1_2017]", cs.provided, "f1_2017").empty(),
          "the plain entrant_list default needs no slice record");
    check(slices::missing_slice("", cs.provided).empty() && slices::missing_slice("art/tile.png", cs.provided).empty() &&
              slices::missing_slice("#overrides.home_screen.tiles_solo_6x6[]", cs.provided).empty(),
          "an image string that names no slice is left alone");
    auto ref = slices::parse_ref("#overrides.teams.driver_select_team_logo[cs17_toro_rosso]");
    check(ref && ref->array == "teams.driver_select_team_logos" && ref->name == "cs17_toro_rosso",
          "a reference reads the array in the game's spelling and the slice name");

    if (argc > 1) {
        std::ifstream f(std::string(argv[1]) + "/ui.texture_slice/examples/cs_era_and_logo.json", std::ios::binary);
        std::stringstream text;
        text << f.rdbuf();
        Value doc = swofty::json::parse(text.str(), nullptr);
        const swofty::json::Object *records = doc["ui.texture_slice"].if_object();
        std::vector<std::pair<std::string, Value>> example;
        std::set<std::string> present;
        for (auto &[key, v] : *records) {
            example.push_back({"swofty.classic-seasons-2017/" + key, v});
            present.insert("swofty.classic-seasons-2017/" + std::string(v["image"].as_string()));
        }
        auto lowered = lower(example, present);
        check(records && example.size() == 2 && lowered.refused == 0 && lowered.images.size() == 2 &&
                  lowered.arrays.size() == 2 && lowered.provided.count("home_screen.tiles_solo_6x6|cs_f1_2017") &&
                  lowered.provided.count("teams.driver_select_team_logos|cs17_toro_rosso"),
              "the docs' Classic Seasons example records both lower, the logo one by the singular array name the docs used");
    }

    std::printf("%s\n", failures ? "FAILED" : "ok");
    return failures ? 1 : 0;
}
