#define ZSTD_STATIC_LINKING_ONLY
#include "core/apply/content/box_ui.hpp"
#include "core/apply/content/garage_build.hpp"

#include <cstdio>
#include <cstring>

namespace content = swofty_core::apply::content;
namespace bu = content::box_ui;
namespace g = content::garage;
namespace ef = content::erpfile;
using Value = swofty::json::Value;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

static g::Image bc1(uint32_t w, uint32_t h, uint8_t seed)
{
    g::Image img;
    img.format = g::FMT_BC1_SRGB;
    img.width = w;
    img.height = h;
    img.levels = 1;
    img.pixels.resize(g::chain_bytes(img.format, w, h, 1));
    for (size_t i = 0; i < img.pixels.size(); i++)
        img.pixels[i] = char(seed + i * 7);
    return img;
}

struct Parsed {
    std::string bytes;
    ef::Source src;
    ef::Archive arc;
    std::string why;
    bool ok = false;
    explicit Parsed(std::string b) : bytes(std::move(b))
    {
        src.size = bytes.size();
        src.read = [this](uint64_t off, size_t n, char *out) {
            if (off + n > bytes.size())
                return false;
            std::memcpy(out, bytes.data() + off, n);
            return true;
        };
        ok = ef::open(src, arc, why);
    }
    Parsed(const Parsed &) = delete;
    std::string frag(const std::string &name, size_t i)
    {
        const ef::Entry *e = arc.find(name);
        std::string out, w;
        if (!e || i >= e->frags.size() || !ef::fragment(src, arc, e->frags[i], out, w))
            return {};
        return out;
    }
};

static std::string rewritten(Parsed &stock, const ef::Changes &changes)
{
    std::string out, why;
    bool ok = ef::rewrite(
        stock.src, stock.arc, changes,
        [&](const char *p, size_t n) {
            out.append(p, n);
            return true;
        },
        why);
    return ok ? out : std::string();
}

int main()
{
    const std::string root = bu::DEFAULT_ROOT;
    const std::string logo = "osd/textures/message_system/ms_fia_logo", other = "textures/broadcast_ui_generic/bc_logos/other";
    const std::string surface = root + logo + ".tif.image", view = root + logo + ".tif",
                      other_surface = root + other + ".tif.image";

    check(bu::resident("2025_asset_groups/ui_package/persistent.erp") &&
              bu::resident("2025_asset_groups/ui_package/flow_persistent.erp") &&
              bu::resident("2025_asset_groups/ui_package/screens/persistent.erp") &&
              bu::resident("2025_asset_groups/ui_package/fonts_efigs_r_p.erp"),
          "persistent.erp, flow_persistent.erp, screens/persistent.erp and the font archives are resident");
    check(!bu::resident("2025_asset_groups/ui_package/flow_race.erp") &&
              !bu::resident("2025_asset_groups/ui_package/screens/broadcast_persistent.erp"),
          "flow_race.erp and screens/broadcast_persistent.erp are not");

    Value rec = swofty::json::parse(R"({"package": "vmknolle.season-1950/f5_1950", "textures": "ui/f5_1950",
        "archives": ["2025_asset_groups/ui_package/flow_race.erp", "2025_asset_groups\\ui_package\\Screens\\Broadcast_Persistent.erp",
                     "2025_asset_groups/ui_package/flow_race.erp"]})");
    bu::Record r = bu::parse("vmknolle.season-1950/f5_1950_ui", rec);
    check(r.ok() && r.addon == "vmknolle.season-1950" && r.package == "vmknolle.season-1950/f5_1950" &&
              r.folder == "ui/f5_1950" && r.root == root && r.srgb,
          "a folder record parses with the default root and sRGB");
    check(r.archives.size() == 2 && r.archives[1] == "2025_asset_groups/ui_package/screens/broadcast_persistent.erp",
          "archive paths are lowered, use / and are listed once");
    Value bad = swofty::json::parse(R"({"package": "a/b", "textures": {"erp": "ui/x.erp", "resources": ["eaid://x.tif.image"]},
        "archives": ["2025_asset_groups/environment_package/tracks/monza/monza_common.erp", "2025_asset_groups/ui_package/../databases.erp"]})");
    bu::Record b = bu::parse("mod/x", bad);
    check(!b.ok() && b.lines.size() == 3 && b.erp == "ui/x.erp" && b.resources.size() == 1,
          "archives outside ui_package and paths that leave it are refused, and then no archive is left");
    check(!bu::parse("mod/y", swofty::json::parse(R"({"package": "a/b", "textures": 3, "archives": []})")).ok(),
          "textures that are neither a folder nor an archive are refused");

    bu::Record other_box = r;
    other_box.id = "mod/other";
    other_box.package = "mod/other_box";
    std::vector<bu::Record> all = {other_box, r};
    check(bu::for_box({"vmknolle.season-1950/f5_1950_tt", "vmknolle.season-1950/f5_1950"}, all).size() == 1 &&
              bu::for_box({"vmknolle.season-1950/f5_1950_tt", "vmknolle.season-1950/f5_1950"}, all)[0].id == r.id,
          "a variant box takes the records of the first package along its chain that has any");
    check(bu::for_box({"stock/f1_2025"}, all).empty(), "a box with no record swaps nothing");

    g::Built stock_built =
        g::build({{logo + ".dds", g::write_dds(bc1(8, 8, 1))}, {other + ".dds", g::write_dds(bc1(4, 4, 2))}}, root, true, false);
    g::Built pay_built = g::build({{logo + ".dds", g::write_dds(bc1(16, 8, 50))}}, root, true, false);
    Parsed stock(stock_built.erp), pay(pay_built.erp);
    check(stock.ok && pay.ok && stock.arc.entries.size() == 4 && pay.arc.entries.size() == 2, "the test archives open");

    bu::Plan plan;
    std::string why;
    check(bu::plan(stock.arc, pay.src, pay.arc, {}, plan, why) && plan.swapped.size() == 2 && plan.lines.empty(),
          "a texture the archive holds is swapped with its view");
    std::string copy = rewritten(stock, plan.changes);
    Parsed out(copy);
    check(out.ok && out.arc.entries.size() == 4, "the copy opens and keeps every resource: " + out.why);
    check(out.frag(surface, 1) == pay.frag(surface, 1) && out.frag(surface, 0) == pay.frag(surface, 0) &&
              out.frag(view, 0) == pay.frag(view, 0),
          "the copy's surface header, pixels and view are the textures' ones");
    check(out.frag(other_surface, 1) == stock.frag(other_surface, 1), "a resource the textures do not name is the stock one");

    bu::Plan half;
    check(bu::plan(stock.arc, pay.src, pay.arc, {surface}, half, why) && half.swapped.empty() && half.lines.size() == 1 &&
              half.lines[0].find("a surface and its view go together") != std::string::npos,
          "a surface listed without its view is kept: " + (half.lines.empty() ? std::string("no line") : half.lines[0]));

    g::Built with_desc = g::build({{logo + ".dds", g::write_dds(bc1(16, 8, 50))}}, root, true, true);
    Parsed desc(with_desc.erp);
    bu::Plan frags;
    check(bu::plan(stock.arc, desc.src, desc.arc, {}, frags, why) && frags.swapped.empty() && frags.lines.size() == 1 &&
              frags.lines[0].find("fragment(s)") != std::string::npos,
          "a surface with another fragment count is kept: " + (frags.lines.empty() ? std::string("no line") : frags.lines[0]));

    g::Built elsewhere = g::build({{"textures/zenith/f1_logo.dds", g::write_dds(bc1(8, 8, 9))}}, root, true, false);
    Parsed none(elsewhere.erp);
    bu::Plan empty;
    check(bu::plan(stock.arc, none.src, none.arc, {}, empty, why) && empty.swapped.empty() && empty.lines.empty(),
          "textures the archive does not hold swap nothing");

    std::string name = bu::copy_name("2025_asset_groups/ui_package/screens/broadcast_persistent.erp", 0x0123456789abcdefull);
    check(name == "boxui_0123456789abcdef_screens_broadcast_persistent.erp", "the copy is named by its key and archive: " + name);
    check(bu::mix(1, "a") != bu::mix(1, "b") && bu::mix(bu::mix(1, "ab"), "") != bu::mix(bu::mix(1, "a"), "b"),
          "the key mixes each part with a separator");

    std::printf("%s\n", failures ? "box_ui_test: FAILED" : "box_ui_test: all passed");
    return failures ? 1 : 0;
}
