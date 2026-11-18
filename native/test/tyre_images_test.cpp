#ifndef NOMINMAX
#define NOMINMAX
#endif
#include "../src/core/badge_archive.hpp"
#include <cassert>
#include <cstdio>

using namespace badge_images;
namespace uidb = swofty_core::apply::content::uidb;
static uidb::Node object() { uidb::Node n; n.kind = uidb::Node::object; return n; }
static uidb::Node text(const std::string &s) { uidb::Node n; n.val.type = "stri"; n.val.value = s; return n; }
static void put(uidb::Node &node, const std::string &key, uidb::Node value)
{
    uidb::Val k; k.type = "stri"; k.value = key;
    node.items.emplace_back(std::move(k), std::move(value));
}
static uidb::Node catalogue(const std::vector<std::string> &names, const std::string &uri)
{
    auto node = object();
    put(node, "TextureArrayName", text(uri));
    uidb::Node rows; rows.kind = uidb::Node::array;
    for (const auto &name : names) {
        auto row = object(); put(row, "Name", text(name)); rows.list.push_back(std::move(row));
    }
    put(node, "Contents", std::move(rows));
    auto contents = object(); put(contents, "key_icons", std::move(node));
    auto allocation = object(); put(allocation, "Contents", std::move(contents));
    contents = object(); put(contents, "tyre_allocation", std::move(allocation));
    auto overrides = object(); put(overrides, "Contents", std::move(contents));
    auto root = object(); put(root, "overrides", std::move(overrides));
    return root;
}
static void slices()
{
    const auto &descriptor = live_image_reference::TYRES[0];
    std::vector<std::string> fragments(2);
    fragments[0].resize(40); fragments[1].resize(32);
    for (auto [at, v] : {std::pair{8, 55u}, {12, 4u}, {16, 4u}, {24, 1u}, {28, 2u}})
        std::memcpy(fragments[0].data() + at, &v, 4);
    // Transparent red then opaque green BC3 slices.
    fragments[1][9] = char(-8); fragments[1][11] = char(-8);
    fragments[1][16] = char(-1); fragments[1][17] = char(-1);
    fragments[1][24] = char(-32); fragments[1][25] = 7;
    fragments[1][26] = char(-32); fragments[1][27] = 7;
    std::string read_uri;
    auto read = [&](const std::string &uri, std::vector<std::string> &out) {
        read_uri = uri; out = fragments; return true;
    };
    const std::string custom = "ui_package/art/texture_arrays/modded/tyres.texarray";
    archive::Catalogue c(descriptor);
    assert(c.load(catalogue({"Soft", "Wet"}, custom), read));
    assert(read_uri == "eaid://" + custom + ".image");
    archive::Pixels pixels;
    const std::string prefix = live_image_reference::prefix(descriptor);
    assert(c.decode(prefix + "Soft", pixels) && pixels.rgba[0] == 255 && pixels.rgba[3] == 0);
    assert(c.decode(prefix + "Wet", pixels) && pixels.rgba[1] == 255 && pixels.rgba[3] == 255);
    assert(!c.decode(prefix + "Missing", pixels));
    assert(!c.decode("#overrides.tyre_allocation.tyre_icons.Soft", pixels));
    // The UIDB order, rather than a compiled-in compound index, selects the slice.
    assert(c.load(catalogue({"Wet", "Soft"}, custom), read));
    assert(c.decode(prefix + "Soft", pixels) && pixels.rgba[1] == 255);
    assert(c.load(catalogue({"Soft", "Soft"}, custom), read));
    assert(!c.decode(prefix + "Soft", pixels));
    // Extra, unreferenced slices do not invalidate the catalogue. A missing
    // slice affects only the row that requests it, as in the badge reader.
    assert(c.load(catalogue({"Soft"}, custom), read));
    assert(c.decode(prefix + "Soft", pixels) && pixels.rgba[0] == 255);
    assert(c.load(catalogue({"Soft", "Wet", "Hard"}, custom), read));
    assert(c.decode(prefix + "Wet", pixels) && pixels.rgba[1] == 255);
    assert(!c.decode(prefix + "Hard", pixels));
    assert(!c.load(catalogue({"Soft", "Wet"}, "../../outside.texarray"), read));
    assert(!live_image_reference::catalogue(prefix + "../Soft"));
    assert(!live_image_reference::catalogue(prefix));
    std::puts("PASS: mod array URI, UIDB slice order, alpha, missing/duplicate names and unsafe resources");
}

static void installed(const char *game)
{
    archive::Env env; env.root = game;
    archive::Resources resources(env);
    assert(resources.available());
    for (const auto &descriptor : live_image_reference::TYRES) {
        archive::Catalogue c(descriptor);
        assert(c.load(resources));
        for (const char *name : {"Soft", "Medium", "Hard", "Intermediate", "Wet", "Soft_f1_26"}) {
            archive::Pixels p;
            assert(c.decode(live_image_reference::prefix(descriptor) + name, p));
            assert(p.width && p.height && p.width <= 64 && p.height <= 64);
            assert(p.rgba.size() == size_t(p.width) * p.height * 4);
            std::printf("DECODE %s.%s %ux%u\n", descriptor.path, name, p.width, p.height);
        }
    }
    archive::Catalogue badges;
    assert(badges.load(resources));
    std::puts("PASS: installed BC7 tyre catalogues and existing badge catalogue");
}
int main(int argc, char **argv)
{
    slices();
    if (argc == 2) installed(argv[1]);
}
