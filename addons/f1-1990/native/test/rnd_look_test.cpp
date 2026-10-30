// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <fstream>
#include <sstream>

#include "rnd_look.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static f1990::Look parse(const std::string &json)
{
    std::vector<f1990::StringLeaf> leaves;
    f1990::Look look;
    if (f1990::parse_string_leaves(json, leaves))
        f1990::read_look(leaves, look);
    return look;
}

int main(int argc, char **argv)
{
    uint32_t abgr = 0;
    check("#RRGGBB to ABGR", f1990::parse_rgb("#2F74E8", abgr) && abgr == 0xffe8742fu);
    check("lower case hex", f1990::parse_rgb("#ff7800", abgr) && abgr == 0xff0078ffu);
    check("rejects a short colour", !f1990::parse_rgb("#FFF", abgr));
    check("rejects a missing #", !f1990::parse_rgb("2F74E8x", abgr));
    check("rejects a bad digit", !f1990::parse_rgb("#2G74E8", abgr));

    std::string full = R"({"package": "#overrides.career.f1990_rnd", "departments": {
        "0": {"name": "aero", "colour": "#2F74E8", "light": "#ACC7F6"},
        "1": {"name": "chassis", "colour": "#22A45D", "light": "#A7DBBE"},
        "2": {"name": "maintenance", "colour": "#F6C21C", "light": "#FBE7A4"},
        "3": {"name": "engine", "colour": "#E2231A", "light": "#F3A7A3"},
        "4": {"name": "tyres", "colour": "#FF7800", "light": "#FFC999"}}})";
    f1990::Look look = parse(full);
    check("complete look", look.complete() && look.package == "#overrides.career.f1990_rnd");
    check("aero colour", look.colour[0] == 0xffe8742fu && look.light[0] == 0xfff6c7acu);
    check("durability is code 2", look.colour[2] == 0xff1cc2f6u);
    check("engine is code 3", look.colour[3] == 0xff1a23e2u);
    check("tyres keeps the rnd-tyres orange and light", look.colour[4] == 0xff0078ffu && look.light[4] == 0xff99c9ffu);

    std::string missing = full;
    missing.replace(missing.find("\"light\": \"#FFC999\""), 18, "\"light\": \"nope\"");
    check("a bad colour leaves the look incomplete", !parse(missing).complete());
    std::string foreign = full;
    foreign.replace(foreign.find("#overrides.career.f1990_rnd"), 27, "#overrides.lobby.f1990_rndx");
    check("a package outside overrides.career is refused", !parse(foreign).complete());

    uint32_t c = 0;
    check("department 4 has a colour", f1990::look_colour(look, 4, false, c) && c == 0xff0078ffu);
    check("light colour", f1990::look_colour(look, 1, true, c) && c == look.light[1]);
    check("department 5 keeps the game's colour", !f1990::look_colour(look, 5, false, c));
    check("a sign-extended department keeps the game's colour", !f1990::look_colour(look, uint64_t(-1), false, c));

    const std::string ours = look.package;
    check("2026 package is a career package", f1990::is_career_package("#overrides.career.rnd_om_xm", ours));
    check("plain career package", f1990::is_career_package("#overrides.career", ours));
    check("ours is not swapped again", !f1990::is_career_package(ours, ours));
    check("another career-prefixed name is not one", !f1990::is_career_package("#overrides.career_end_icons", ours));
    check("other packages are left", !f1990::is_career_package("#overrides.team_hq", ours));

    check("department tab path",
          f1990::dept_icon_path("#overrides.career.rnd_om_xm.rnd_dept_icons_small.aero", ours) ==
              "#overrides.career.f1990_rnd.rnd_dept_icons_small.aero");
    check("rnd-tyres' Tyres tab path",
          f1990::dept_icon_path("#overrides.career.rnd_om_xm.rnd_dept_icons_small.tyres", ours) ==
              "#overrides.career.f1990_rnd.rnd_dept_icons_small.tyres");
    check("plain package tab path",
          f1990::dept_icon_path("#overrides.career.rnd_dept_icons_small.maintenance", ours) ==
              "#overrides.career.f1990_rnd.rnd_dept_icons_small.maintenance");
    check("other atlases are left",
          f1990::dept_icon_path("#overrides.career.rnd_om_xm.rnd_part_image.om_xm_calipers", ours).empty());
    check("an already swapped path is left",
          f1990::dept_icon_path("#overrides.career.f1990_rnd.rnd_dept_icons_small.aero", ours).empty());

    check("pie department slot", f1990::pie_slot_is_department("aero") && f1990::pie_slot_is_department("tyres"));
    check("pie shutdown slot keeps its colour", !f1990::pie_slot_is_department("shutdown"));
    check("pie regulation slot keeps its colour", !f1990::pie_slot_is_department("regulation"));

    if (argc > 1) {
        std::ifstream f(argv[1]);
        std::stringstream ss;
        ss << f.rdbuf();
        f1990::Look shipped = parse(ss.str());
        check("shipped data/rnd-look.json is complete", shipped.complete());
        check("shipped Tyres colour is the rnd-tyres orange", shipped.colour[4] == 0xff0078ffu);
    }

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
