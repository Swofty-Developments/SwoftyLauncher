// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the qualifying engine upgrades (rnd lane: f1990_quali_rev_limit, f1990_quali_engine)
// count only in qualifying, in 1990 careers only, and outside qualifying every car runs the race engine.
// Its own DLL.
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
//
// Race engine: the handling files carry the qualifying rating (the period power figures). The fill
// also sets a flag for the session (on outside qualifying in a 1990 career, off in qualifying, in
// any other career and when a career unloads). While it is on, site_Car_EngineDriveTorque (the drive
// torque per full throttle in Car_Powertrain_Step, rbx = the Engine system, xmm7 = rad/s) scales the
// torque of every car by race_engine::kPower and cuts it over the last 60 rpm before a limit 300 rpm
// under the engine's own (Engine +0x2528).
//
// F1 1990 box (Grand Prix and Time Trial, no career loaded): at each session start
// (Sess_TyreAllocation_OnSessionStart), when every entrant of the session's line-up drives a 1990 car, the
// race engine is on in practice (session ids 1 to 4) and the race (15 to 17) and off in qualifying (5 to
// 14) and Time Trial (18). Every other session start without a career, and every session end without one,
// turns it off, so other boxes keep the stock engines.

#include <atomic>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "box_session.hpp"
#include "career_1990.hpp"
#include "race_engine.hpp"
#include "rnd_progress.hpp"

SWOFTY_SETTING(bool, quali_engine_1990, true);

namespace {

using f1990::at;

constexpr uint8_t kEnginePower = 0;
constexpr size_t kWeekendVt = 0x98 / 8;

thread_local bool t_leave_out;
std::atomic<uint32_t> g_fills, g_changes;
std::atomic<bool> g_race_engine{false};

void set_race_engine(bool on, const char *why)
{
    if (g_race_engine.exchange(on) != on)
        swofty::log("[f1-1990] quali engine: race engine %s (%s): power x%.4f, limit %.0f rpm lower", on ? "on" : "off",
                    why, double(f1990::race_engine::kPower), double(f1990::race_engine::kLimiterDropRpm));
}

void on_career(const SwoftyEvent *, void *)
{
    set_race_engine(false, "career loaded or unloaded");
}

void on_session_end(const SwoftyEvent *, void *)
{
    if (!swofty::career::entity())
        set_race_engine(false, "session ended outside a career");
}

void subscribe(const char *event, SwoftyEventFn fn = on_career)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, fn, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] quali engine: subscribe %s failed", event);
}

std::atomic<uint32_t> g_box_logged;

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
            if (group >= 0)
                set_race_engine(leave_out, leave_out ? "1990 practice or race" : "1990 qualifying");
            if (g_fills++ < 8)
                swofty::log("[f1-1990] quali engine: car %u set up for a session of group %d; qualifying engine lines %s",
                            car, group, group < 0 ? "as stock (no session)" : leave_out ? "left out" : "counted");
        } else if (career) {
            set_race_engine(false, "not a 1990 career");
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

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    if (!quali_engine_1990 || !component || swofty::career::entity())
        return;
    void *session = f1990::box::session_of(component);
    uint32_t id = session ? at<uint32_t>(session, 0x08) : 0;
    auto lineup = f1990::box::lineup(component);
    bool box = session && lineup.all_1990();
    bool race = box && (f1990::box::practice(id) || f1990::box::race(id));
    if (g_box_logged++ < 16)
        swofty::log("[f1-1990] quali engine: session %u without a career: %s (%zu of %zu entrants on 1990 cars)", id,
                    !lineup.readable ? "line-up not readable, stock engines"
                    : !box           ? "not the F1 1990 box, stock engines"
                    : race           ? "F1 1990 box, race engine"
                                     : "F1 1990 box, qualifying engine",
                    lineup.on_1990, lineup.entrants);
    set_race_engine(race, race ? "F1 1990 box practice or race" : box ? "F1 1990 box qualifying or Time Trial"
                                                                      : "not the F1 1990 box");
}

SWOFTY_SITE(site_Car_EngineDriveTorque, swofty::Context &c)
{
    if (!g_race_engine.load(std::memory_order_relaxed))
        return;
    auto *engine = reinterpret_cast<const swofty::game::CarEngine *>(c.rbx);
    float limiter = engine ? engine->limiter_rotation_rate : 0.f;
    c.xmm0.f32[0] *= f1990::race_engine::torque_scale(c.xmm7.f32[0], limiter);
}

SWOFTY_ON_LOAD
{
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
    subscribe(SWOFTY_EVENT_SESSION_END, on_session_end);
}

SWOFTY_ADDON()
