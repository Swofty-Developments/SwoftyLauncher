// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - no mandatory stop in the F1 1990 box (its own DLL).
//
// A 1990 career runs on the f1_1990 setup mode, whose tyre_regulations rows (tools/rules.py) switch the strategy
// stop and valid-lap checks and the race-length lists of the two-compound rule off. The box runs on the shared
// swofty_f1 mode, which Classic Seasons and F1 2012 use with the rule on. Sess_Parse_GameSetupMode (0x142a14f20)
// keeps the mode's tyre_regulations in the GameSetupMode object: +0x88 allow_strategy_mandatory_stop_check,
// +0x89 allow_strategy_valid_laps_check, +0x8a tyre_setup_enabled, and the race0, race1 and race2 length flags in
// the bytes from +0x8b. At every session start in the box the DLL clears +0x88, +0x89 and the length flags of the
// live swofty_f1 mode and keeps the originals; at the next session start outside the box it writes them back.

#include <cstdio>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "box_gate.hpp"
#include "no_stop.hpp"

SWOFTY_SETTING(bool, box_no_stop, true);

namespace {

f1990::StopRules g_rules;
uintptr_t g_mode;

bool writable_mode(uintptr_t mode)
{
    return mode && swofty::mem::readable(mode, f1990::StopRules::kLengthsEnd);
}

}

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    bool box = box_no_stop && f1990::box_list_selected();
    if (box) {
        uintptr_t mode = f1990::box_host_mode();
        if (writable_mode(mode) && g_rules.relax(reinterpret_cast<uint8_t *>(mode))) {
            g_mode = mode;
            swofty::log("[f1-1990] no stop: F1 1990 box session, the mandatory stop checks and the race length lists of the "
                        "host mode are off");
        }
    } else if (g_rules.held && writable_mode(g_mode)) {
        g_rules.restore(reinterpret_cast<uint8_t *>(g_mode));
        swofty::log("[f1-1990] no stop: not the F1 1990 box, the host mode's tyre regulations are back");
        g_mode = 0;
    }
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] no stop: %s", box_no_stop ? "on" : "off (box_no_stop)");
}

SWOFTY_ADDON()
