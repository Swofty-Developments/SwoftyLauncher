// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - twelve laps per car in each qualifying session (its own DLL, so a binding it can't resolve
// drops only this rule).
//
// In a 1990 weekend's Friday and Saturday qualifying (session ids 5..14), every car may start 12 laps.
// Leaving the garage starts one (pit status 13, LeavingGarage, through Game_Pitlane_SetVehicleStatus),
// and so does each line crossing (session.lap) while the car is on track or driving through the pit
// lane, at least 45 s after the car's last counted start, so a flashback or a car rocking over the line
// counts once. A car that has started 12 and crosses the line again, or leaves its garage or its pit box
// again, goes back to its pit box through Game_PitLane_PutVehicleInPitBox, the call the red flag makes,
// and on into the garage when the pit lane's forced-box byte (+0x300) is set; the call runs on the next
// session frame (Sess_InGame_UpdateBase), outside the progress update that posts the crossing. The player sees a race HUD toast with the laps left at each start. The rule
// exists only where the f1_1990 setup mode's weekend runs; setting quali_laps_1990 turns it off.

#include <windows.h>

#include <atomic>
#include <cstdio>
#include <mutex>
#include <vector>

#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/hud.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/systems/sessions.hpp>

#include "quali_laps.hpp"
#include "weekend.hpp"

SWOFTY_SETTING(bool, quali_laps_1990, true);

namespace {

using f1990::at;
namespace q = f1990::qlaps;

constexpr size_t kEntrantSize = 0x88;
constexpr size_t kEntrantMax = 256;
constexpr int kLogAi = 8;
constexpr int kLogSendIn = 64;

std::mutex g_lock;
std::atomic<bool> g_active{false};
std::atomic<bool> g_pending{false};
q::Counter g_counter;
void *g_pitlane;
uint32_t g_session;
uint8_t g_player = 0xff;
std::vector<uint8_t> g_queue;
int g_ai_lines, g_send_lines;

double now()
{
    return double(GetTickCount64()) / 1000.0;
}

void *call_getter(void *obj, size_t slot)
{
    auto **vt = *static_cast<void ***>(obj);
    return vt && vt[slot] ? reinterpret_cast<void *(*)(void *)>(vt[slot])(obj) : nullptr;
}

uint8_t player_vehicle(void *component)
{
    auto *c = static_cast<uint8_t *>(component);
    if (!swofty::mem::readable(c + 0x278, 8))
        return 0xff;
    auto *career = at<uint8_t *>(c, 0x278);
    if (!career || !swofty::mem::readable(career + 0x2e0, 0x10))
        return 0xff;
    auto *b = at<const uint8_t *>(career, 0x2e0), *e = at<const uint8_t *>(career, 0x2e8);
    if (!b || e <= b || size_t(e - b) % kEntrantSize || size_t(e - b) / kEntrantSize > kEntrantMax ||
        !swofty::mem::readable(b, size_t(e - b)))
        return 0xff;
    for (; b < e; b += kEntrantSize)
        if ((at<uint8_t>(b, 0x4b) & 0x10) && at<uint8_t>(b, 0x82) != 0xff)
            return at<uint8_t>(b, 0x82);
    return 0xff;
}

uint32_t live_status(void *pitlane, uint8_t v, uint32_t fallback)
{
    auto *slot = static_cast<uint8_t *>(pitlane) + 0x68 + size_t(v) * 0x10;
    if (!swofty::mem::readable(slot, 8))
        return fallback;
    auto *car = at<uint8_t *>(slot, 0);
    if (!car || !swofty::mem::readable(car + 4, 4))
        return fallback;
    uint32_t s = at<uint32_t>(car, 4);
    return s <= 16 ? s : fallback;
}

void toast(uint8_t v, q::Act act)
{
    if (v != g_player)
        return;
    int left = g_counter.left(v), used = g_counter.car(v).used;
    char text[64], detail[96];
    if (act == q::Act::send_in) {
        std::snprintf(text, sizeof text, "NO LAPS LEFT");
        std::snprintf(detail, sizeof detail, "All %d qualifying laps used: back to the garage", q::kLimit);
    } else {
        std::snprintf(text, sizeof text, "LAP %d OF %d", used, q::kLimit);
        if (act == q::Act::last)
            std::snprintf(detail, sizeof detail, "Last qualifying lap: box at the end of it");
        else
            std::snprintf(detail, sizeof detail, "%d qualifying lap%s left after this one", left, left == 1 ? "" : "s");
    }
    auto r = swofty::hud::toast(text, detail, act == q::Act::counted ? 3.0f : 5.0f,
                                act == q::Act::send_in ? "chequered" : "");
    if (r != swofty::hud::Result::ok)
        swofty::log("[f1-1990] quali laps: toast '%s' not shown: %s", text, swofty::hud::name(r));
}

void act_on(uint8_t v, q::Act act, const char *what)
{
    if (act == q::Act::none)
        return;
    const q::Car &c = g_counter.car(v);
    bool player = v == g_player;
    if (act == q::Act::send_in) {
        if (g_counter.queue(v)) {
            g_queue.push_back(v);
            g_pending = true;
        }
        if (g_send_lines++ < kLogSendIn)
            swofty::log("[f1-1990] quali laps: session %u: V%02u%s %s with all %d laps used; sent to its garage on the "
                        "next frame",
                        g_session, v, player ? " (player)" : "", what, q::kLimit);
    } else if (player || act == q::Act::last || g_ai_lines++ < kLogAi) {
        swofty::log("[f1-1990] quali laps: session %u: V%02u%s %s: lap %d of %d started", g_session, v,
                    player ? " (player)" : "", what, c.used, q::kLimit);
    }
    toast(v, act);
}

void drain()
{
    std::vector<uint8_t> list;
    void *pitlane;
    {
        std::lock_guard lock(g_lock);
        list.swap(g_queue);
        g_pending = false;
        pitlane = g_pitlane;
        for (uint8_t v : list)
            g_counter.sent(v);
    }
    if (!pitlane || !g_active)
        return;
    for (uint8_t v : list) {
        uint32_t s = live_status(pitlane, v, q::kUnknown);
        if (!q::out_of_garage(s)) {
            swofty::log("[f1-1990] quali laps: session %u: V%02u is already on its way in (pit status %u); not sent",
                        g_session, v, s);
            continue;
        }
        swofty::game::Game_PitLane_PutVehicleInPitBox(pitlane, v);
        auto *box = static_cast<uint8_t *>(pitlane) + 0x300;
        int forced = swofty::mem::readable(box, 1) ? *box : -1;
        swofty::log("[f1-1990] quali laps: session %u: V%02u put in its pit box (pit status %u -> %u, forced box %d%s)",
                    g_session, v, s, live_status(pitlane, v, q::kUnknown), forced,
                    forced > 0 ? ": the crew pushes it into the garage" : ": it stays in the box until released");
    }
}

void on_lap(const swofty::sessions::events::Lap &e, const swofty::events::Context &)
{
    if (!g_active)
        return;
    uint32_t v = e.car.index();
    if (v >= q::kCars)
        return;
    std::lock_guard lock(g_lock);
    act_on(uint8_t(v), g_counter.crossing(uint8_t(v), now()), "crossed the line");
}

void on_unloaded(const SwoftyEvent *, void *)
{
    std::lock_guard lock(g_lock);
    g_active = false;
    g_pitlane = nullptr;
    g_queue.clear();
    g_pending = false;
}

}

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    if (!component)
        return;
    void *provider = at<void *>(component, 0x270);
    void *session = provider ? call_getter(provider, 0xa8 / 8) : nullptr;
    void *weekend = provider ? call_getter(provider, 0x98 / 8) : nullptr;
    uint32_t id = session ? at<uint32_t>(session, 0x08) : 0;
    bool on = quali_laps_1990 && session && weekend && f1990::is_1990_weekend(weekend) && q::qualifying_session(id);
    std::lock_guard lock(g_lock);
    g_counter.reset();
    g_queue.clear();
    g_pending = false;
    g_pitlane = nullptr;
    g_session = id;
    g_ai_lines = g_send_lines = 0;
    g_player = on ? player_vehicle(component) : 0xff;
    g_active = on;
    if (on)
        swofty::log("[f1-1990] quali laps: session %u: %d laps per car, player V%02u", id, q::kLimit, g_player);
}

SWOFTY_WRAP(Game_Pitlane_SetVehicleStatus, void, void *self, uint8_t vehicle, uint32_t status)
{
    orig(self, vehicle, status);
    if (!g_active || !self || vehicle >= q::kCars)
        return;
    std::lock_guard lock(g_lock);
    g_pitlane = self;
    uint32_t s = live_status(self, vehicle, status);
    act_on(vehicle, g_counter.status(vehicle, s, now()), s == q::kLeavingPitBox ? "left its pit box" : "left its garage");
}

SWOFTY_WRAP(Sess_InGame_UpdateBase, uint32_t *, void *state, uint32_t *route, double dt)
{
    uint32_t *r = orig(state, route, dt);
    if (g_pending.load(std::memory_order_relaxed))
        drain();
    return r;
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] quali laps loaded: %d laps per car in 1990 qualifying %s", q::kLimit,
                quali_laps_1990 ? "on" : "off (quali_laps_1990)");
    auto r = swofty::events::on<swofty::sessions::events::Lap>().after(on_lap);
    if (r != swofty::events::Result::ok)
        swofty::log("[f1-1990] quali laps: no session.lap (%s); only garage exits count", swofty::events::name(r));
    if (swofty::events::subscribe(SWOFTY_EVENT_CAREER_UNLOADED, on_unloaded) != swofty::events::Result::ok)
        swofty::log("[f1-1990] quali laps: no career.unloaded event");
}

SWOFTY_ADDON()
