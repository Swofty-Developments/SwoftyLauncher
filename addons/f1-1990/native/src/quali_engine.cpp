// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the qualifying engine upgrades (rnd lane: f1990_quali_rev_limit, f1990_quali_engine)
// count only in qualifying, in 1990 careers only. Its own DLL.
//
// Game_Rnd_FillVehicleUpgrades fills a car's vehicle upgrade component when the car is set up for a
// session, with one Game_Rnd_CalcUpgradeTypeProgress per upgrade type. The two qualifying upgrades
// are Engine Power (type 0) instances, so outside qualifying, while the loaded career's data group
// is f1_career_1990, the progress of type 0 inside that fill is recomputed with them left out of
// the completed weighting (the denominator, scale and clamp as the game's). Qualifying is the
// current session of the weekend (provider vt +0x98, session list +0x220, current index +0x238)
// in group 1 (Qualifying) or 2 (SprintShootout, the Friday session). Every other caller of the
// progress function, and every other career, gets the game's value. Option quali_engine_1990
// turns it off.

#include <atomic>

#include <swofty/career.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "career_1990.hpp"
#include "rnd_progress.hpp"

SWOFTY_SETTING(bool, quali_engine_1990, true);

namespace {

using f1990::at;

constexpr uint8_t kEnginePower = 0;
constexpr size_t kWeekendVt = 0x98 / 8;

thread_local bool t_leave_out;
std::atomic<uint32_t> g_fills, g_changes;

int current_group(void *component)
{
    auto *provider = at<void *>(component, 0x270);
    auto **vt = provider && swofty::mem::readable(provider, 8) ? *static_cast<void ***>(provider) : nullptr;
    if (!vt || !swofty::mem::readable(vt + kWeekendVt, 8) || !vt[kWeekendVt])
        return -1;
    auto *weekend = reinterpret_cast<void *(*)(void *)>(vt[kWeekendVt])(provider);
    if (!weekend || !swofty::mem::readable(weekend, 0x240))
        return -1;
    return f1990::session_group(weekend, at<uint32_t>(weekend, 0x238));
}

}

SWOFTY_WRAP(Game_Rnd_FillVehicleUpgrades, void, void *component, uint8_t car, uint8_t a3, uint8_t a4)
{
    bool leave_out = false;
    if (quali_engine_1990 && component) {
        void *career = swofty::career::entity();
        if (career && f1990::is_1990_career(career)) {
            int group = current_group(component);
            leave_out = group >= 0 && !f1990::is_qualifying_group(group);
            if (g_fills++ < 8)
                swofty::log("[f1-1990] quali engine: car %u set up for a session of group %d; qualifying engine lines %s",
                            car, group, group < 0 ? "as stock (no session)" : leave_out ? "left out" : "counted");
        }
    }
    t_leave_out = leave_out;
    orig(component, car, a3, a4);
    t_leave_out = false;
}

SWOFTY_WRAP(Game_Rnd_CalcUpgradeTypeProgress, float, void *const *instances, uint8_t type, uint8_t state, const void *meta)
{
    float stock = orig(instances, type, state, meta);
    if (!t_leave_out || type != kEnginePower || !instances || !meta)
        return stock;
    size_t count = size_t(at<uint64_t>(instances, 0xc00));
    float p = f1990::type_progress(instances, count, type, state, at<const uint8_t *>(meta, 0x08),
                                   at<const uint8_t *>(meta, 0x10), f1990::is_quali_upgrade);
    if (p != stock && g_changes++ < 8)
        swofty::log("[f1-1990] quali engine: Engine Power progress %.3f -> %.3f outside qualifying", double(stock),
                    double(p));
    return p;
}

SWOFTY_ADDON()
