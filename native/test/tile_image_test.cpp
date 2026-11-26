// Host unit test for the ui.tile image choice (native/src/core/frontend/tile_image_logic.hpp): which template a tile gets and
// what its image leaf is set to. Built and run by native/test/run_tests.sh tile_image.
#include "core/frontend/tile_image_logic.hpp"

#include <cstdio>
#include <cstring>
#include <string>

namespace ti = frontend::tile_image;
namespace tr = frontend::tile_records;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static tr::Tile tile(const std::string &image)
{
    tr::Tile t;
    t.id = "a/t";
    t.title = "T";
    t.image = image;
    return t;
}

int main()
{
    tr::SliceOf none;
    auto known = [](const std::string &id) -> std::optional<swofty_core::apply::content::slices::Ref> {
        if (id == "a/logo")
            return swofty_core::apply::content::slices::Ref{"home_screen.tiles_multiplayer_6x12", "a_logo"};
        return std::nullopt;
    };

    auto p = ti::plan(tile(""), known, true, false);
    expect(p.step == ti::Step::none && !p.template_name && p.path.empty(),
           "a tile with no image keeps the stock template and no leaf");

    p = ti::plan(tile("#home_screen.tiles_multiplayer_6x12[x]"), none, true, false);
    expect(p.step == ti::Step::set && p.path == "#overrides.home_screen.tiles_multiplayer_6x12[x]",
           "a #array[name] image is the path the registry resolves");
    expect(std::strcmp(p.template_name, ti::HOME_TEMPLATE) == 0, "a home tile with an image gets the image template");

    p = ti::plan(tile("a/logo"), known, true, true);
    expect(p.step == ti::Step::set && p.path == "#overrides.home_screen.tiles_multiplayer_6x12[a_logo]",
           "a slice record id resolves through the slice table");
    expect(std::strcmp(p.template_name, ti::ROW_TEMPLATE) == 0,
           "a tile in the career Addons row gets the row layout of the image template");

    p = ti::plan(tile("a/logo"), known, false, false);
    expect(p.step == ti::Step::template_missing && !p.template_name && !p.path.empty(),
           "no image template in the build: the stock template stays and the tile shows without an image");

    p = ti::plan(tile("a/nothing"), known, true, false);
    expect(p.step == ti::Step::unresolved && !p.why.empty() && !p.template_name,
           "an image naming no slice is logged and leaves the leaf unset");
    p = ti::plan(tile("a/nothing"), none, true, false);
    expect(p.step == ti::Step::unresolved, "no slice table: a bare id is unresolved");

    ti::Counts c;
    c.set = 2;
    c.template_missing = 1;
    expect(ti::summary("home", c) == "[ui.tile] home: tile images: 2 given their image, 1 without the UI build's image template",
           "the summary names each kind");
    expect(ti::summary("career", {}) == "[ui.tile] career: tile images: none", "an empty summary says none");

    printf("%s\n", fails ? "FAILED" : "all ok");
    return fails ? 1 : 0;
}
