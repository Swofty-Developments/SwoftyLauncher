// Host-native unit test for the ui.tile record logic (native/src/core/frontend/tile_records.hpp): parsing, place filter, dedupe
// by id, gate over the facts, the cap, title and image resolution, and a walk of the records in test/content/tiles/tiles.json
// in load order. Built and run by native/test/run_tests.sh tile_records.

#include "core/frontend/tile_records.hpp"

#include <cstdio>
#include <fstream>
#include <sstream>

namespace tr = frontend::tile_records;
namespace hk = swofty_core::hooks;
using swofty::json::Value;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static Value parse(const char *text)
{
    swofty::json::Error err;
    return swofty::json::parse(text, &err);
}

static std::string ids(const tr::Result &r)
{
    std::string out;
    for (auto &t : r.tiles)
        out += (out.empty() ? "" : ",") + t.id;
    return out;
}

static bool has_line(const tr::Result &r, const char *part)
{
    for (auto &l : r.lines)
        if (l.find(part) != std::string::npos)
            return true;
    return false;
}

struct Set {
    std::vector<std::pair<std::string, Value>> records;
    std::vector<tr::Input> inputs() const
    {
        std::vector<tr::Input> in;
        for (auto &[id, v] : records)
            in.push_back({id, &v});
        return in;
    }
    void add(const char *id, const char *json) { records.emplace_back(id, parse(json)); }
};

static hk::Facts facts(const char *mode, const char *content)
{
    hk::Facts f;
    f.mode = mode;
    f.content = content;
    f.chain = {content};
    return f;
}

int main(int argc, char **argv)
{
    Set s;
    s.add("a/one", R"({"place":"home","title":"lng_one","route":"r1"})");
    s.add("a/two", R"({"place":"home","title":{"literal":"Two"},"route":"r2"})");
    s.add("b/world", R"({"place":"f1_world","title":"lng_w","route":"rw"})");
    s.add("b/three", R"({"place":"home","title":"lng_three","route":"r3"})");
    hk::Facts none;

    auto home = tr::effective(s.inputs(), tr::Place::home, &none);
    expect(ids(home) == "a/one,a/two,b/three" && home.other_place == 1,
           "records of the place come back in load order, other places left out");
    expect(ids(tr::effective(s.inputs(), tr::Place::f1_world, &none)) == "b/world", "another place gets its own records");
    expect(ids(tr::effective(s.inputs(), tr::Place::career, &none)).empty(), "a place with no records gets none");

    expect(!home.tiles[0].literal && tr::title_text(home.tiles[0]).localised &&
               std::string(tr::title_text(home.tiles[0]).text) == "lng_one",
           "a lang key title is localised");
    expect(home.tiles[1].literal && !tr::title_text(home.tiles[1]).localised &&
               std::string(tr::title_text(home.tiles[1]).text) == "Two",
           "a literal title is not localised");

    Set dup;
    dup.add("a/x", R"({"place":"home","title":"lng_old","route":"r"})");
    dup.add("a/y", R"({"place":"home","title":"lng_y","route":"r"})");
    dup.add("a/x", R"({"place":"home","title":"lng_new","route":"r"})");
    auto d = tr::effective(dup.inputs(), tr::Place::home, &none);
    expect(ids(d) == "a/x,a/y" && d.tiles[0].title == "lng_new" && d.deduped == 1 && has_line(d, "a/x: listed twice"),
           "a repeated id keeps its first position and the later content");

    Set gated;
    gated.add("g/all", R"({"place":"home","title":"lng_a","route":"r"})");
    gated.add("g/career", R"({"place":"home","title":"lng_b","route":"r","gate":{"mode":"career"}})");
    gated.add("g/both", R"({"place":"home","title":"lng_c","route":"r","gate":[{"mode":"career"},{"content":"mod/c13"}]})");
    gated.add("g/null", R"({"place":"home","title":"lng_d","route":"r","gate":null})");
    gated.add("g/empty", R"({"place":"home","title":"lng_e","route":"r","gate":[]})");
    hk::Facts menu = facts("main_menu", "");
    hk::Facts career = facts("career", "mod/other");
    hk::Facts season = facts("career", "mod/c13");
    expect(ids(tr::effective(gated.inputs(), tr::Place::home, &menu)) == "g/all,g/null,g/empty",
           "a gated tile stays out of the menu");
    expect(ids(tr::effective(gated.inputs(), tr::Place::home, &career)) == "g/all,g/career,g/null,g/empty",
           "a mode gate shows in that mode only, a list needs every selector");
    auto sr = tr::effective(gated.inputs(), tr::Place::home, &season);
    expect(ids(sr) == "g/all,g/career,g/both,g/null,g/empty" && sr.gated_out == 0,
           "every selector of a list matching shows the tile");
    auto nf = tr::effective(gated.inputs(), tr::Place::home, nullptr);
    expect(ids(nf) == "g/all,g/null,g/empty" && nf.gated_out == 2, "with no facts a gated tile does not show");

    Set bad;
    bad.add("b/place", R"({"place":"nowhere","title":"lng"})");
    bad.add("b/title", R"({"place":"home","route":"r"})");
    bad.add("b/gate", R"({"place":"home","title":"lng","route":"r","gate":{"no_such":1}})");
    bad.add("b/ok", R"({"place":"home","title":"lng","route":"r"})");
    auto b = tr::effective(bad.inputs(), tr::Place::home, &none);
    expect(ids(b) == "b/ok" && b.refused == 3 && has_line(b, "b/place: not used: place") &&
               has_line(b, "b/title: not used: title is missing") && has_line(b, "b/gate: not used: gate does not compile"),
           "a record with a bad place, title or gate is refused with its reason");

    tr::Options need;
    need.need_route = true;
    Set nr;
    nr.add("n/a", R"({"place":"home","title":"lng"})");
    auto n = tr::effective(nr.inputs(), tr::Place::home, &none, need);
    expect(n.tiles.empty() && n.refused == 1 && has_line(n, "needs a route"),
           "a place that needs a route refuses a tile without one");
    expect(tr::effective(nr.inputs(), tr::Place::home, &none).tiles.size() == 1, "a route is optional unless the place needs it");

    Set many;
    for (int i = 0; i < 300; i++)
        many.records.emplace_back("m/t" + std::to_string(i), parse(R"({"place":"home","title":"lng","route":"r"})"));
    auto m = tr::effective(many.inputs(), tr::Place::home, &none);
    expect(m.tiles.size() == tr::TILE_CAP && m.capped == 45 && m.tiles.back().id == "m/t254" &&
               has_line(m, "45 tile(s) over the cap of 255"),
           "the cap keeps the first 255 in load order and says how many it left out");
    tr::Options small;
    small.cap = 2;
    expect(tr::effective(many.inputs(), tr::Place::home, &none, small).tiles.size() == 2, "the cap is the caller's room left");

    namespace slices = swofty_core::apply::content::slices;
    tr::SliceOf lookup = [](const std::string &id) -> std::optional<slices::Ref> {
        if (id == "mod.a/tile")
            return slices::Ref{"home_screen.tiles_solo_6x6", "cs_a"};
        return std::nullopt;
    };
    tr::Tile t;
    t.image = "mod.a/tile";
    auto img = tr::resolve_image(t, lookup);
    expect(img.kind == tr::Image::Kind::slice && img.array == "home_screen.tiles_solo_6x6" && img.name == "cs_a",
           "an image naming a texture slice record resolves to its array and name");
    t.image = "#overrides.teams.driver_select_team_logo[x]";
    img = tr::resolve_image(t, lookup);
    expect(img.kind == tr::Image::Kind::slice && img.array == "teams.driver_select_team_logos" && img.name == "x",
           "a #array[name] image resolves with the canonical array");
    t.image = "mod.a/missing";
    expect(tr::resolve_image(t, lookup).kind == tr::Image::Kind::unresolved && !tr::resolve_image(t, lookup).why.empty(),
           "an image that names nothing is unresolved and says so");
    t.image.clear();
    expect(tr::resolve_image(t, lookup).kind == tr::Image::Kind::none, "no image is none");

    if (argc > 1) {
        std::ifstream f(argv[1], std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        Value doc = parse(ss.str().c_str());
        expect(doc["ui.tile"].is_object(), "the fixture parses");
        Set fx;
        if (const swofty::json::Object *obj = doc["ui.tile"].if_object())
            for (auto &[id, v] : *obj)
                fx.records.emplace_back(id, v);
        auto walk = [&](tr::Place p, const hk::Facts *fa, const tr::Options &o) { return tr::effective(fx.inputs(), p, fa, o); };
        auto home_menu = walk(tr::Place::home, &menu, {});
        expect(ids(home_menu) == "mod.a/settings,mod.b/no_route,mod.c/settings,mod.c/hash_image",
               "fixture: the menu shows the ungated home tiles that parse, in file order");
        expect(home_menu.refused == 3 && has_line(home_menu, "mod.c/no_title") && has_line(home_menu, "mod.c/bad_gate"),
               "fixture: the no-title and bad-gate records are refused");
        tr::Options r;
        r.need_route = true;
        hk::Facts sw13 = facts("career", "mod.b/sw_2013");
        auto routed = walk(tr::Place::home, &sw13, r);
        expect(ids(routed) == "mod.a/settings,mod.a/career_only,mod.b/season,mod.c/settings,mod.c/hash_image",
               "fixture: in the season the gated tiles join, a tile with no route is left out");
        expect(ids(walk(tr::Place::f1_world, &menu, {})) == "mod.b/world",
               "fixture: the F1 World place reads only its own record");
    }

    return failed ? 1 : 0;
}
