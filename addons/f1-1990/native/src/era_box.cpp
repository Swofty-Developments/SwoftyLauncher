// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the F1 1990 box in the Addons row of the Grand Prix and Time Trial era picker. Its own DLL.
//
// The box selects f1_1990_gp (the 1990 grid, 22 or 26 cars as the cars setting) on the Grand Prix picker
// and f1_1990_tt (one car per team, 13) on the Time Trial picker. db/box.json adds both lists to the
// core's host setup mode swofty_f1, their scenarios and the f1_1990 track layer for swofty_f1.

#include <swofty/era.hpp>
#include <swofty/mixin.hpp>

namespace {

constexpr const char *kGrandPrix = "f1_1990_gp";
constexpr const char *kTimeTrial = "f1_1990_tt";

bool init()
{
    if (!swofty::era::available()) {
        swofty::log("[f1-1990] box: this core has no era picker boxes; no F1 1990 box in Grand Prix or Time Trial");
        return true;
    }
    auto r = swofty::era::add_box({.entrant_list = kGrandPrix,
                                   .title = "lng_f1990_box",
                                   .image = "#overrides.home_screen.tiles_solo_6x6[f1]",
                                   .order = 1990,
                                   .time_trial = kTimeTrial});
    swofty::log("[f1-1990] box: F1 1990 box %s (Grand Prix %s, Time Trial %s)",
                r == swofty::era::Result::ok ? "registered" : "refused", kGrandPrix, kTimeTrial);
    return true;
}

}

SWOFTY_ADDON(init)
