// Host-native unit test for the F1 World list append logic (native/src/core/frontend/f1_world_logic.hpp): which build of
// FeTile_Build_List is the F1 World hub, the next button index derived from the items vector, the cap room, and the
// f1_world record filtering that feeds the append. Built and run by native/test/run_tests.sh f1_world_logic.

#include "core/frontend/f1_world_logic.hpp"

#include <cstdio>

namespace fw = frontend::f1_world;
namespace tr = frontend::tile_records;
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

int main()
{
    expect(fw::is_world_list(3, 0), "menu 3 sub 0 is the F1 World hub");
    expect(!fw::is_world_list(0, 0), "menu 0 (home) is not");
    expect(!fw::is_world_list(0, 7), "menu 0 sub 7 (F1 World + Local MP strip) is not");
    expect(!fw::is_world_list(3, 1), "menu 3 with another sub kind is not");
    expect(!fw::is_world_list(6, 0), "menu 6 (career content) is not");
    expect(!fw::is_world_list(4, 0), "menu 4 (showroom) is not");

    expect(fw::items_in(0x1000, 0x1060) == 6u, "six stock entries: (end - begin) / 0x10 = 6");
    expect(fw::items_in(0x1000, 0x1000) == 0u, "an empty vector has no items");
    expect(!fw::items_in(0, 0x60), "a null begin is not a vector");
    expect(!fw::items_in(0x1060, 0x1000), "end before begin is refused");
    expect(!fw::items_in(0x1000, 0x1058), "a partial entry is refused");

    expect(fw::next_index(6) == 7, "after the six stock tiles the next button is button_7");
    expect(fw::next_index(0) == 1, "an empty list starts at button_1");
    expect(fw::next_index(254) == 255, "button_255 is the last index");
    expect(!fw::next_index(255), "a full list has no next index");

    expect(fw::room_for(6) == 249, "room for 249 more after six");
    expect(fw::room_for(255) == 0, "no room at the cap");
    expect(fw::room_for(400) == 0, "no room over the cap");

    Value a = parse(R"({"place":"f1_world","title":"lng_a","route":"mod/a"})");
    Value b = parse(R"({"place":"home","title":"lng_b","route":"mod/b"})");
    Value c = parse(R"({"place":"f1_world","title":{"literal":"C"},"route":"mod/c"})");
    Value d = parse(R"({"place":"f1_world","title":"lng_d"})");
    Value e = parse(R"({"place":"f1_world","title":"lng_e","route":"mod/e"})");
    std::vector<tr::Input> in{{"mod/a", &a}, {"mod/b", &b}, {"mod/c", &c}, {"mod/d", &d}, {"mod/e", &e}};

    tr::Options opt;
    opt.need_route = true;
    opt.cap = fw::room_for(6);
    auto r = tr::effective(in, tr::Place::f1_world, nullptr, opt);
    expect(r.tiles.size() == 3 && r.tiles[0].id == "mod/a" && r.tiles[1].id == "mod/c" && r.tiles[2].id == "mod/e",
           "f1_world keeps its own records with a route, in load order");
    expect(r.other_place == 1 && r.refused == 1, "the home record is another place, the routeless one is refused");
    expect(r.tiles[1].literal && !r.tiles[0].literal, "a literal title stays literal");

    opt.cap = fw::room_for(254);
    r = tr::effective(in, tr::Place::f1_world, nullptr, opt);
    expect(r.tiles.size() == 1 && r.capped == 2, "with one slot left the cap drops the rest");
    expect(fw::next_index(254 + 0) == 255, "and that one tile gets button_255");

    opt.cap = fw::room_for(255);
    r = tr::effective(in, tr::Place::f1_world, nullptr, opt);
    expect(r.tiles.empty() && r.capped == 3, "a full list shows no record tile");

    return failed ? 1 : 0;
}
