// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>

#include "rnd_art.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

int main()
{
    std::string_view base;
    check("no suffix for an available card", f1990::state_suffix("f1990_carbon_tub", base).empty() && base == "f1990_carbon_tub");
    check("splits _in_progress", f1990::state_suffix("f1990_carbon_tub_in_progress", base) == "_in_progress" &&
                                     base == "f1990_carbon_tub");
    check("splits _complete",
          f1990::state_suffix("f1990_carbon_tub_complete", base) == "_complete" && base == "f1990_carbon_tub");

    const char *donor = "om_xm_monocoque_structure";
    check("om_xm image", f1990::donor_frame(donor, "", true, false) == "om_xm_monocoque_structure");
    check("om_xm in progress", f1990::donor_frame(donor, "_in_progress", true, false) == "om_xm_monocoque_structure_progress");
    check("om_xm complete", f1990::donor_frame(donor, "_complete", true, false) == "om_xm_monocoque_structure_complete");
    check("om_xm mask", f1990::donor_frame(donor, "", true, true) == "om_xm_monocoque_structure_m");
    check("plain in progress", f1990::donor_frame(donor, "_in_progress", false, false) == "monocoque_structure_in_progress");
    check("plain mask", f1990::donor_frame(donor, "", false, true) == "monocoque_structure");
    check("bare donor", f1990::donor_frame("calipers", "_complete", true, false) == "om_xm_calipers_complete");

    check("2026 R&D package", f1990::is_rnd_package(f1990::kRnDPackage) && f1990::package_is_omxm(f1990::kRnDPackage));
    check("pre-2026 R&D package", f1990::is_rnd_package("#overrides.career") &&
                                      !f1990::package_is_omxm("#overrides.career"));
    check("an empty package is not one", !f1990::is_rnd_package(""));
    check("a career-prefixed list is not one", !f1990::is_rnd_package("#overrides.career_end_icons"));
    check("another package is not one", !f1990::is_rnd_package("#overrides.team_hq"));

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
