// Host-native unit test for the ui.tile record logic (native/src/core/frontend/tile_records.hpp): parsing, place filter, dedupe
// by id, gate over the facts, the cap, title and image resolution, and a walk of the records in test/content/tiles/tiles.json
// in load order. Built and run by native/test/run_tests.sh tile_records.

#include "core/frontend/chosen_tiles.hpp"
#include "core/frontend/tile_records.hpp"

#include <cstdio>
#include <fstream>
#include <set>
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

    {
        std::set<std::string> routes{"mod.settings/open_settings"};
        tr::RouteExists exists = [&](const std::string &id) { return routes.count(id) > 0; };
        expect(tr::game_route("mod.settings/open_settings", exists) == "open_settings",
               "route: a ui.route record id becomes the key the build cloned the group under");
        expect(tr::game_route("mod_menu", exists) == "mod_menu" && tr::game_route("open_settings", exists) == "open_settings",
               "route: a stock route name or a bare key passes through unchanged");
        bool unknown = false;
        expect(tr::game_route("mod.gone/open", exists, &unknown) == "mod.gone/open" && unknown,
               "route: an id no enabled record has passes through as written and is flagged");
        expect(tr::game_route("mod.settings/open_settings", {}) == "mod.settings/open_settings",
               "route: no lookup leaves it alone");
        Set rt;
        rt.add("a/ok", R"({"place":"home","title":"lng","route":"mod.settings/open_settings"})");
        rt.add("a/stock", R"({"place":"home","title":"lng","route":"career_hub"})");
        rt.add("a/typo", R"({"place":"home","title":"lng","route":"mod.settings/open_setings"})");
        tr::Options with;
        with.route_exists = exists;
        auto r = tr::effective(rt.inputs(), tr::Place::home, &none, with);
        expect(r.tiles.size() == 3 && r.tiles[0].route == "open_settings" && r.tiles[1].route == "career_hub" &&
                   r.tiles[2].route == "mod.settings/open_setings" && has_line(r, "a/typo: route") && !has_line(r, "a/ok: route"),
               "route: record tiles get the game's route name, a stale id is reported");
    }

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

    std::set<std::string> have{"home_screen.tiles_solo_6x6|cs_a"}, told;
    t.id = "mod.a/home";
    t.image = "#overrides.home_screen.tiles_solo_6x6[cs_a]";
    expect(tr::missing_slice_warning(t, tr::resolve_image(t, lookup), have, told).empty(),
           "a #array[name] image that a ui.texture_slice record provides does not warn");
    t.image = "#overrides.home_screen.tiles_solo_6x6[cs_typo]";
    std::string warn = tr::missing_slice_warning(t, tr::resolve_image(t, lookup), have, told);
    expect(warn.find("mod.a/home: image slice cs_typo of home_screen.tiles_solo_6x6 is not provided") == 0,
           "a #array[name] image no record provides warns with the slice and array");
    expect(tr::missing_slice_warning(t, tr::resolve_image(t, lookup), have, told).empty(), "the same slice warns once");
    t.image = "mod.a/tile";
    expect(tr::missing_slice_warning(t, tr::resolve_image(t, lookup), have, told).empty(),
           "an image naming a slice record is provided by definition");
    t.image = "art/tile.png";
    expect(tr::missing_slice_warning(t, tr::resolve_image(t, lookup), have, told).empty(), "a plain string is not checked");

    t.image = "mod.a/slice_nope";
    auto nope = tr::resolve_image(t, lookup);
    expect(nope.kind == tr::Image::Kind::unresolved &&
               nope.why == "image slice mod.a/slice_nope is not provided by any enabled ui.texture_slice record",
           "an image that names a slice record no addon provides is kept as unresolved with the documented warning");
    t.image = "nonsense";
    expect(tr::resolve_image(t, lookup).why.find("neither a ui.texture_slice record id nor a #array[name]") != std::string::npos,
           "an image that is no record id and no #array[name] says what it may be");

    {
        std::set<std::string> routes{"mod.settings/open_settings"};
        tr::RouteExists exists = [&](const std::string &id) { return routes.count(id) > 0; };
        namespace ct = frontend::chosen_tiles;
        std::vector<tr::Tile> list;
        ct::Chosen chosen{list, 10, tr::Place::home};
        ct::ChosenTable table(chosen);
        auto add = [&](const char *id, const char *route) {
            SwoftyTileSpec spec{};
            spec.id = id;
            spec.title = "lng";
            spec.route = route;
            return table.table()->insert(table.table()->self, 0, &spec);
        };
        expect(add("h/ok", "mod.settings/open_settings") == SWOFTY_ASSET_OK &&
                   add("h/typo", "mod.settings/open_setings") == SWOFTY_ASSET_OK &&
                   add("h/stock", "mod_menu") == SWOFTY_ASSET_OK && add("h/bare", "stdui_nonexistent") == SWOFTY_ASSET_OK,
               "a handler can insert tiles with a record id, a typo, a stock and a bare route");
        std::vector<std::string> lines;
        ct::settle(list, 10, exists, &lines);
        std::string typo;
        for (auto &l : lines)
            typo += l + "\n";
        expect(lines.size() == 1 &&
                   lines[0] == "ui.tile h/typo: route mod.settings/open_setings names no enabled ui.route record; the game "
                               "gets it as written",
               "settle reports a handler-inserted route id that no enabled ui.route record has, once");
        bool keyed = false;
        for (auto &t : list)
            keyed = keyed || (t.id == "h/ok" && t.route == "open_settings");
        expect(keyed, "settle turns a handler-inserted record id into the key");
        lines.clear();
        ct::settle(list, 10, exists, &lines);
        expect(lines.empty(), "a tile already told about is not reported again");

        Set rt;
        rt.add("a/typo", R"({"place":"home","title":"lng","route":"mod.settings/open_setings"})");
        tr::Options with;
        with.route_exists = exists;
        auto r = tr::effective(rt.inputs(), tr::Place::home, &none, with);
        expect(r.lines.size() == 1 && r.tiles.size() == 1 && r.tiles[0].route_told,
               "a record tile with an unknown route id is kept and reported by effective");
        lines.clear();
        ct::settle(r.tiles, 10, exists, &lines);
        expect(lines.empty(), "settle does not report it a second time");
    }

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
        expect(ids(walk(tr::Place::career, &menu, r)) == "mod.a/career_tile",
               "fixture: the career place shows its ungated tile; the one with no route is refused");
        expect(ids(walk(tr::Place::career, &sw13, r)) == "mod.a/career_tile,mod.b/career_gated",
               "fixture: a gated career tile joins in its content");
        tr::Options room;
        room.need_route = true;
        room.cap = 2;
        auto cat = walk(tr::Place::car_category, &menu, room);
        expect(ids(cat) == "mod.a/cat_one,mod.b/cat_two" && cat.capped == 1 && has_line(cat, "over the cap of 2"),
               "fixture: car category tiles keep load order and the slots left cap the rest, logged");
        room.cap = 0;
        auto none = walk(tr::Place::car_category, &menu, room);
        expect(none.tiles.empty() && none.capped == 3, "fixture: no slot left shows no car category tile");
    }

    return failed ? 1 : 0;
}
