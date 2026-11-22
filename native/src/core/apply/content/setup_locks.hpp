#pragma once

#include <swofty/json.hpp>

#include <array>
#include <cstdint>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

// Setup sliders a box locks (content.package 1.8 rules.setup_locked, on with SWOFTY_BOX_SETUP=1). The game keeps one
// 64-bit lock mask per setup entry ([entry+8]): bit n locks slider n, the internal slider index of the car_setup.sliders
// name table (Gdb_Load_SetupSliders_Body 0x149a29f90). Every setter returns without a change when the bit is set
// (Car_Setup_SetCustomSliderStep 0x140c112b0, Car_Setup_ResetParamToZeroStep 0x140c11430), the setup screen's slider
// reads the same bit (0x14178a800), and parc ferme sets bits {1, 7-17, 25, 28} into it
// (Game_CarSetupMgr_OnParcFermeStateChange 0x140c0abc0). Car_Setup_ApplyTuning ignores it, so a locked slider keeps
// driving the car with the step it has. Pure, for the host tests.

namespace swofty_core::apply::content {

using Value = swofty::json::Value;

inline constexpr const char *SETUP_LOCKS_ENV = "SWOFTY_BOX_SETUP";

struct SetupLockLabel {
    const char *label;
    uint64_t bits;
};

constexpr uint64_t slider_bit(int i) { return uint64_t(1) << i; }
constexpr uint64_t slider_bits(int first, int last)
{
    uint64_t b = 0;
    for (int i = first; i <= last; ++i)
        b |= slider_bit(i);
    return b;
}

// Labels to slider bits. Indices 0-18 and 25-28 follow the name table order the KB gives (L2): 0 front wing, 1 rear
// wing, 2/3 diff on/off throttle, 4 mid corner, 5 high speed, 6 preload, 7/8 camber, 9/10 toe, 11/12 springs, 13/14
// ride height, 15/16 anti-roll bars, 17 brake pressure, 18 bias, 25 ballast, 26 fuel, 27 engine braking, 28 ramp
// diff. Where the order inside a group is not read (brake migration 19, 20, 41; dry pressures 21-24; inter and wet
// pressures 29-36; cooling and ducts 37-40) only the whole group has a label.
inline constexpr std::array<SetupLockLabel, 28> SETUP_LOCK_LABELS = {{
    {"front_wing", slider_bit(0)},
    {"rear_wing", slider_bit(1)},
    {"on_throttle", slider_bit(2)},
    {"off_throttle", slider_bit(3)},
    {"mid_corner", slider_bit(4)},
    {"diff_high_speed", slider_bit(5)},
    {"diff_preload", slider_bit(6)},
    {"differential", slider_bits(2, 6) | slider_bit(28)},
    {"front_camber", slider_bit(7)},
    {"rear_camber", slider_bit(8)},
    {"front_toe", slider_bit(9)},
    {"rear_toe", slider_bit(10)},
    {"front_suspension", slider_bit(11)},
    {"rear_suspension", slider_bit(12)},
    {"front_suspension_height", slider_bit(13)},
    {"rear_suspension_height", slider_bit(14)},
    {"front_antiroll_bar", slider_bit(15)},
    {"rear_antiroll_bar", slider_bit(16)},
    {"brake_pressure", slider_bit(17)},
    {"brake_bias", slider_bit(18)},
    {"brake_migration", slider_bits(19, 20) | slider_bit(41)},
    {"tyre_pressures", slider_bits(21, 24)},
    {"tyre_pressures_inter_wet", slider_bits(29, 36)},
    {"ballast", slider_bit(25)},
    {"fuel_load", slider_bit(26)},
    {"engine_braking_fraction", slider_bit(27)},
    {"ramp_differential", slider_bit(28)},
    {"brake_cooling", slider_bits(37, 40)},
}};

// The bits of one label; 0 when the label is not one of SETUP_LOCK_LABELS.
inline uint64_t setup_lock_bits(std::string_view label)
{
    for (const auto &l : SETUP_LOCK_LABELS)
        if (label == l.label)
            return l.bits;
    return 0;
}

struct SetupLocksPick {
    std::string package;
    uint64_t mask = 0;
    std::string line;
};

// The locks of the box whose entrant list was pressed: the last content.package (in the order given) with that entrant
// list, and the bits of every known label of its rules.setup_locked. Unknown labels are named in the line.
inline SetupLocksPick setup_locks_pick(const std::string &entrant_list,
                                       const std::vector<std::pair<std::string, const Value *>> &packages)
{
    SetupLocksPick p;
    const Value *found = nullptr;
    for (auto &[id, rec] : packages)
        if (rec && (*rec)["entrant_list"].is_string() && (*rec)["entrant_list"].as_string() == entrant_list) {
            p.package = id;
            found = rec;
        }
    if (!found) {
        p.line = "box " + entrant_list + ": no content.package; the game's own setup locks";
        return p;
    }
    std::string head = "box " + entrant_list + " (" + p.package + "): ";
    const Value &list = (*found)["rules"]["setup_locked"];
    std::string names, unknown;
    if (list.is_array())
        for (size_t i = 0; i < list.size(); ++i) {
            const Value &v = list[i];
            if (!v.is_string())
                continue;
            std::string label(v.as_string());
            uint64_t bits = setup_lock_bits(label);
            std::string &to = bits ? names : unknown;
            to += (to.empty() ? "" : ", ") + label;
            p.mask |= bits;
        }
    if (!p.mask)
        p.line = head + "no setup lock; the game's own setup locks";
    else
        p.line = head + "setup sliders locked: " + names;
    if (!unknown.empty())
        p.line += " (unknown labels left out: " + unknown + ")";
    return p;
}

// The mask an entry gets: the game's own bits plus the box's. `added` are the bits the box set that the game had not.
inline uint64_t setup_locks_merge(uint64_t game, uint64_t box, uint64_t *added = nullptr)
{
    if (added)
        *added = box & ~game;
    return game | box;
}

// SWOFTY_BOX_SETUP=1 turns the locks on; anything else (or no variable) leaves them off.
inline bool setup_locks_enabled(std::string_view env) { return env == "1"; }

}
