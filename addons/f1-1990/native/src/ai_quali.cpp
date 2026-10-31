// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the AI's qualifying pace. Its own DLL, so a binding the core can't resolve drops only this.
//
// The 1990 AI tables are built in race trim (race tyre, race engine). The AI's grip input (vehicle
// state +0x24, from Car_Tyre_ComputeGripMultiplier) carries wear, temperature and the handling
// file's GripMultiplier but not the compound's Peak, so the AI on qualifiers would corner at race
// tyre speeds. In 1990 qualifying (the session group at each car's upgrade fill is 1 or 2, in a
// career whose data group is f1_career_1990) Ai_Eval_CurvatureModifier, the AI's corner radius
// factor, returns its value times kRadiusScale. 0x14068fd10 reads the corner speed from the .copd
// row at that radius, so the scale is the qualifier/race tyre grip ratio (Goodyear 0.97/0.895,
// Pirelli 0.99/0.91): at a grip-limited radius the speed is the qualifier's, about 4 % up. Off in
// every other session, in other careers, and when a career loads or unloads. Braking (.brpd) and
// traction (.acpd) lookups do not see it. The power gap comes from the cars' own engines
// (f1_1990_quali.dll). Option quali_engine_1990 covers it.

#include <atomic>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "career_1990.hpp"
#include "rnd_progress.hpp"

namespace swofty::game {
using Ai_Eval_CurvatureModifier_1990_t = float(float grip, float mass_bias, void *settings, bool flag, void *line_a,
                                               float dist_a, void *line_b, float dist_b);
inline constexpr fn<"Ai_Eval_CurvatureModifier", Ai_Eval_CurvatureModifier_1990_t> Ai_Eval_CurvatureModifier_1990{};
}

SWOFTY_SETTING(bool, quali_engine_1990, true);

namespace {

using f1990::at;

constexpr float kRadiusScale = 1.085f;
constexpr size_t kWeekendVt = 0x98 / 8;

std::atomic<bool> g_quali{false};
std::atomic<uint32_t> g_calls;

void set_quali(bool on, const char *why)
{
    if (g_quali.exchange(on) != on)
        swofty::log("[f1-1990] ai quali: %s (%s): AI corner radius x%.3f", on ? "on" : "off", why,
                    double(on ? kRadiusScale : 1.f));
}

void on_career(const SwoftyEvent *, void *)
{
    set_quali(false, "career loaded or unloaded");
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] ai quali: subscribe %s failed", event);
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
                set_quali(f1990::is_qualifying_group(group), f1990::is_qualifying_group(group)
                                                                 ? "1990 qualifying"
                                                                 : "1990 practice or race");
        } else if (career) {
            set_quali(false, "not a 1990 career");
        }
    }
    orig(component, car, a3, a4);
}

SWOFTY_WRAP(Ai_Eval_CurvatureModifier_1990, float, float grip, float mass_bias, void *settings, bool flag, void *line_a,
            float dist_a, void *line_b, float dist_b)
{
    float m = orig(grip, mass_bias, settings, flag, line_a, dist_a, line_b, dist_b);
    if (!g_quali.load(std::memory_order_relaxed))
        return m;
    if (g_calls.fetch_add(1, std::memory_order_relaxed) == 0)
        swofty::log("[f1-1990] ai quali: first scaled corner: radius factor %.3f -> %.3f", double(m),
                    double(m * kRadiusScale));
    return m * kRadiusScale;
}

SWOFTY_ON_LOAD
{
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
}

SWOFTY_ADDON()
