// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the AI's tyre pace. Its own DLL, so a binding the core can't resolve drops only this.
//
// The 1990 AI tables are built in qualifying trim (the qualifier, the qualifying engine). The AI's
// grip input (vehicle state +0x24, from Car_Tyre_ComputeGripMultiplier) carries wear, temperature
// and the handling file's GripMultiplier but not the compound's Peak, so on race tyres the AI would
// still read the qualifier rows. 0x14063e370 loads that grip once per car per update and hands it to
// the coordinate builders of all six tables (acpd, brpd, copd, crpd and two more); at
// site_Ai_TableGripInput it is in xmm1. Outside 1990 qualifying (the session group at each car's
// upgrade fill is known and not 1 or 2, in a career whose data group is f1_career_1990) the dry grip
// (weather byte cl 0, the tables' first weather row) is scaled by kRaceGrip, so cornering, braking and
// traction read the rows that match the race tyre (0.895/0.97 Goodyear, 0.91/0.99 Pirelli of the
// qualifier's grip; the tables' grip rows follow EA's 2026 rows, which lose less pace per unit of grip
// than the car, so the factor is 0.87 rather than 0.92). Off in 1990 qualifying, in other careers and
// modes, and when a career loads or unloads. Option quali_engine_1990 covers it.
//
// F1 1990 box (Grand Prix and Time Trial, no career loaded): at each session start
// (Sess_TyreAllocation_OnSessionStart), when every entrant of the session's line-up drives a 1990 car,
// the race grip is on in practice and the race and off in qualifying (session ids 5 to 14) and Time
// Trial (qualifier trim). Every other session start without a career, and every session end without
// one, turns it off.

#include <atomic>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "box_gate.hpp"
#include "box_session.hpp"
#include "career_1990.hpp"
#include "rnd_progress.hpp"

namespace swofty::game {
inline constexpr site<"site_Ai_TableGripInput"> site_Ai_TableGripInput_1990{};
}

SWOFTY_SETTING(bool, quali_engine_1990, true);

namespace {

using f1990::at;

constexpr float kRaceGrip = 0.87f;
constexpr size_t kWeekendVt = 0x98 / 8;

std::atomic<bool> g_race{false};
std::atomic<uint32_t> g_calls, g_wet;

void set_race(bool on, const char *why)
{
    if (g_race.exchange(on) != on)
        swofty::log("[f1-1990] ai tyres: %s (%s): AI table grip x%.3f", on ? "race tyres" : "qualifiers or stock", why,
                    double(on ? kRaceGrip : 1.f));
}

void on_career(const SwoftyEvent *, void *)
{
    set_race(false, "career loaded or unloaded");
}

void on_session_end(const SwoftyEvent *, void *)
{
    void *career = swofty::career::entity();
    if (!career || !f1990::is_1990_career(career))
        set_race(false, "session ended outside a 1990 career");
}

void subscribe(const char *event, SwoftyEventFn fn = on_career)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, fn, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] ai tyres: subscribe %s failed", event);
}

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
    if (quali_engine_1990 && component) {
        void *career = swofty::career::entity();
        if (career && f1990::is_1990_career(career)) {
            int group = current_group(component);
            if (group >= 0)
                set_race(!f1990::is_qualifying_group(group), f1990::is_qualifying_group(group)
                                                                 ? "1990 qualifying"
                                                                 : "1990 practice or race");
        } else if (career && !f1990::box_list_selected()) {
            set_race(false, "not a 1990 career");
        }
    }
    orig(component, car, a3, a4);
}

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    if (!quali_engine_1990 || !component)
        return;
    void *career = swofty::career::entity();
    if (career && f1990::is_1990_career(career))
        return;
    void *session = f1990::box::session_of(component);
    uint32_t id = session ? at<uint32_t>(session, 0x08) : 0;
    bool box = session && f1990::box_list_selected();
    bool race = box && !f1990::box::qualifying(id) && id != f1990::box::kTimeTrial;
    set_race(race, race ? "F1 1990 box practice or race"
                   : box ? "F1 1990 box qualifying or Time Trial"
                         : "not the F1 1990 box");
}

SWOFTY_SITE(site_Ai_TableGripInput_1990, swofty::Context &c)
{
    if (!g_race.load(std::memory_order_relaxed))
        return;
    uint8_t weather = uint8_t(c.rcx);
    if (weather != 0) {
        if (g_wet.fetch_add(1, std::memory_order_relaxed) == 0)
            swofty::log("[f1-1990] ai tyres: weather byte %u: table grip left as it is", unsigned(weather));
        return;
    }
    float grip = c.xmm1.f32[0];
    c.xmm1.f32[0] = grip * kRaceGrip;
    if (g_calls.fetch_add(1, std::memory_order_relaxed) == 0)
        swofty::log("[f1-1990] ai tyres: first scaled table grip %.3f -> %.3f", double(grip), double(grip * kRaceGrip));
}

SWOFTY_ON_LOAD
{
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
    subscribe(SWOFTY_EVENT_SESSION_END, on_session_end);
}

SWOFTY_ADDON()
