// SPDX-License-Identifier: AGPL-3.0-or-later
//
// League Stewarding (steward.dll): stewarding tools for league racing, run from a browser page on the
// host's own machine. The addon only acts when the game is the session host (role int at session+8 == 1,
// read in Game_Network_SetScenario, and SafetyCarLogic+0x30 == 1).
//
// Penalties: the page's requests are queued and run on the game thread at the top of the rule manager's
// incident processor (Sess_CornerCutting_Update, once per frame in a race). Each kind calls the game's own
// issuer with a zeroed 0x40-byte incident (+0 car, +8 severity, +9 lap, +0x14 seconds or places).
// Automatic penalties are muted by returning early from the issuers while the processor itself is
// running; the addon's own calls come from outside it and pass.
//
// Race control: the safety car state machine is changed in the SafetyCarLogic tick
// (Sess_SafetyCar_VehicleTransition): after the stock tick the addon runs the same exit, index write and
// enter steps the stock transition runs, for an edge that exists in the graph.

#include <atomic>
#include <chrono>
#include <cstring>
#include <deque>
#include <mutex>
#include <string>
#include <vector>

#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "router.hpp"
#include "server.hpp"
#include "telemetry.hpp"
#include "web_assets.hpp"

namespace {

using namespace swofty::game;
using namespace steward;

constexpr uint16_t kTelemetryPort = 20777;
constexpr uintptr_t kSessionRole = 0x8;
constexpr uintptr_t kLogicRole = 0x30;
constexpr uintptr_t kLogicEntriesBegin = 0x48;
constexpr uintptr_t kLogicEntriesEnd = 0x50;
constexpr uintptr_t kLogicStatesBegin = 0x78;
constexpr uintptr_t kLogicStatesEnd = 0x80;
constexpr uintptr_t kStateStride = 0x18;
constexpr uintptr_t kStateObject = 0x10;
constexpr uintptr_t kEntryState = 0x3c;
constexpr uintptr_t kEntryUnchanged = 0x3d;
constexpr uintptr_t kStateEnter = 0x20;
constexpr uintptr_t kStateExit = 0x28;
constexpr size_t kIncidentSize = 0x40;
constexpr int64_t kFreshMs = 3000;
constexpr size_t kMaxLocal = 100;
constexpr size_t kMaxQueue = 32;

int64_t now_ms()
{
    using namespace std::chrono;
    return duration_cast<milliseconds>(steady_clock::now().time_since_epoch()).count();
}

template <class T> T rd(uintptr_t a, T def = T{})
{
    T v{};
    return swofty::mem::read(a, v) ? v : def;
}

std::atomic<int> g_role{-1};
std::atomic<uintptr_t> g_rules{0};
std::atomic<int64_t> g_rules_ms{0};
std::atomic<int> g_rc_state{-1};
std::atomic<bool> g_auto{true};
std::atomic<bool> g_session_seen{false};
std::atomic<bool> g_browser_stale{true};
thread_local bool t_in_auto = false;

std::mutex g_mu;
std::deque<PenaltyAction> g_penalties;
std::deque<RcAction> g_race_control;
std::vector<Event> g_local;
double g_anchor_t = 0;
int64_t g_anchor_ms = 0;
bool g_anchored = false;
int64_t g_start_ms = 0;

bool is_host()
{
    return g_role.load() == 1;
}

bool muted()
{
    return t_in_auto && is_host() && !g_auto.load();
}

void local_event(const char *code, std::string text, int car = -1, int other = -1)
{
    std::lock_guard<std::mutex> g(g_mu);
    int64_t now = now_ms();
    Event e;
    e.t = g_anchored ? g_anchor_t + double(now - g_anchor_ms) / 1000.0 : double(now - g_start_ms) / 1000.0;
    e.code = code;
    e.text = std::move(text);
    e.car = car;
    e.other = other;
    g_local.push_back(std::move(e));
    if (g_local.size() > kMaxLocal)
        g_local.erase(g_local.begin(), g_local.begin() + (g_local.size() - kMaxLocal));
}

std::string describe(const PenaltyAction &a)
{
    std::string what;
    switch (a.kind) {
    case Kind::warning: what = "Warning"; break;
    case Kind::time: what = std::to_string(a.seconds) + " s time penalty"; break;
    case Kind::drive_through: what = "Drive-through penalty"; break;
    case Kind::stop_go: what = "Stop-go penalty"; break;
    case Kind::grid: what = std::to_string(a.places) + " place grid penalty"; break;
    case Kind::dsq: what = "Disqualification"; break;
    }
    what += " for car " + std::to_string(a.car);
    if (!a.text.empty())
        what += ": " + a.text;
    return what;
}

int car_lap(int car)
{
    Snapshot s = telemetry_snapshot();
    for (const Driver &d : s.drivers)
        if (d.index == car)
            return d.lap > 0 && d.lap < 255 ? d.lap : 0;
    return 0;
}

void issue(void *rules, const PenaltyAction &a)
{
    alignas(8) uint8_t incident[kIncidentSize]{};
    incident[0] = uint8_t(a.car);
    incident[9] = uint8_t(car_lap(a.car));
    uint32_t reason = uint32_t(a.reason), extra = 0;
    switch (a.kind) {
    case Kind::warning:
        incident[8] = 1;
        Sess_Issue_Warning(rules, incident, reason);
        break;
    case Kind::time:
        incident[8] = 2;
        extra = uint32_t(a.seconds);
        std::memcpy(incident + 0x14, &extra, 4);
        Sess_Issue_TimePenalty(rules, incident, reason, uint32_t(a.seconds));
        break;
    case Kind::stop_go:
        incident[8] = 3;
        Sess_Issue_StopGo(rules, incident, reason);
        break;
    case Kind::drive_through:
        incident[8] = 4;
        Sess_Issue_DriveThrough(rules, incident, reason);
        break;
    case Kind::grid:
        incident[8] = 3;
        extra = uint32_t(a.places);
        std::memcpy(incident + 0x14, &extra, 4);
        Sess_Issue_GridPlacePenalty(rules, incident, reason, uint32_t(a.places));
        break;
    case Kind::dsq:
        incident[8] = 6;
        Sess_Issue_Disqualify(rules, incident, reason);
        break;
    }
    swofty::log("[steward] issued %s", describe(a).c_str());
    local_event("LOCAL", "Steward: " + describe(a), a.car);
}

void drain_penalties(void *rules)
{
    std::deque<PenaltyAction> todo;
    {
        std::lock_guard<std::mutex> g(g_mu);
        todo.swap(g_penalties);
    }
    for (const PenaltyAction &a : todo)
        issue(rules, a);
}

bool transition(uintptr_t logic, uintptr_t entry, int to)
{
    uintptr_t b = rd<uintptr_t>(logic + kLogicStatesBegin), e = rd<uintptr_t>(logic + kLogicStatesEnd);
    if (!b || e <= b || (e - b) % kStateStride)
        return false;
    size_t count = (e - b) / kStateStride;
    int from = rd<uint8_t>(entry + kEntryState, 0xff);
    if (from >= int(count) || to >= int(count))
        return false;
    uintptr_t old_state = rd<uintptr_t>(b + kStateObject + uintptr_t(from) * kStateStride);
    uintptr_t new_state = rd<uintptr_t>(b + kStateObject + uintptr_t(to) * kStateStride);
    if (!swofty::mem::object_like(old_state) || !swofty::mem::object_like(new_state))
        return false;
    uintptr_t exit_fn = rd<uintptr_t>(rd<uintptr_t>(old_state) + kStateExit);
    uintptr_t enter_fn = rd<uintptr_t>(rd<uintptr_t>(new_state) + kStateEnter);
    if (!swofty::mem::in_image(exit_fn) || !swofty::mem::in_image(enter_fn))
        return false;
    reinterpret_cast<void (*)(uintptr_t, uintptr_t)>(exit_fn)(old_state, entry);
    *reinterpret_cast<uint8_t *>(entry + kEntryState) = uint8_t(to);
    *reinterpret_cast<uint8_t *>(entry + kEntryUnchanged) = uint8_t(from == to);
    reinterpret_cast<void (*)(uintptr_t, uintptr_t)>(enter_fn)(new_state, entry);
    return true;
}

std::vector<uintptr_t> logic_entries(uintptr_t logic)
{
    std::vector<uintptr_t> out;
    uintptr_t b = rd<uintptr_t>(logic + kLogicEntriesBegin), e = rd<uintptr_t>(logic + kLogicEntriesEnd);
    if (!b || e <= b || (e - b) % 8 || (e - b) / 8 > 8)
        return out;
    for (uintptr_t p = b; p < e; p += 8)
        if (uintptr_t entry = rd<uintptr_t>(p); entry && swofty::mem::readable(entry, 0x40))
            out.push_back(entry);
    return out;
}

void run_race_control(uintptr_t logic, RcAction action)
{
    std::vector<uintptr_t> entries = logic_entries(logic);
    if (entries.empty()) {
        local_event("LOCAL", std::string("Steward: ") + rc_action_name(action) + " refused, " + rc_refusal(action, -1));
        return;
    }
    for (uintptr_t entry : entries) {
        int from = rd<uint8_t>(entry + kEntryState, 0xff);
        int to = rc_target(action, from);
        if (to < 0) {
            local_event("LOCAL", std::string("Steward: ") + rc_action_name(action) + " refused, " + rc_refusal(action, from));
            continue;
        }
        bool ok = transition(logic, entry, to);
        swofty::log("[steward] race control %s: state %d -> %d %s", rc_action_name(action), from, to, ok ? "done" : "failed");
        local_event("LOCAL", std::string("Steward: race control ") + rc_action_name(action) + (ok ? "" : " failed"));
    }
}

struct Panel final : Backend {
    std::string state_json() override
    {
        StateView v;
        int role = g_role.load();
        v.role = role == 1 ? "host" : role >= 2 ? "client" : "offline";
        bool racing = now_ms() - g_rules_ms.load() < kFreshMs;
        v.phase = racing ? "race" : g_session_seen.load() ? "unknown" : "lobby";
        int rc = g_rc_state.load();
        v.rc_state = rc_state_name(rc);
        v.rc_raw = rc < 0 ? 0 : rc;
        v.auto_penalties = g_auto.load();
        v.telemetry_port = kTelemetryPort;
        v.snap = telemetry_snapshot();
        std::lock_guard<std::mutex> g(g_mu);
        if (!v.snap.events.empty() && (!g_anchored || v.snap.events.back().t != g_anchor_t)) {
            g_anchored = true;
            g_anchor_t = v.snap.events.back().t;
            g_anchor_ms = now_ms();
        }
        v.local_events = g_local;
        return steward::state_json(v);
    }

    std::string queue_penalty(const PenaltyAction &a) override
    {
        if (!is_host())
            return "this game is not the session host";
        if (now_ms() - g_rules_ms.load() > kFreshMs)
            return "penalties can be issued while a race is running";
        std::lock_guard<std::mutex> g(g_mu);
        if (g_penalties.size() >= kMaxQueue)
            return "too many actions waiting";
        g_penalties.push_back(a);
        return {};
    }

    std::string queue_race_control(RcAction a) override
    {
        if (!is_host())
            return "this game is not the session host";
        int from = g_rc_state.load();
        if (rc_target(a, from) < 0)
            return rc_refusal(a, from);
        std::lock_guard<std::mutex> g(g_mu);
        if (g_race_control.size() >= kMaxQueue)
            return "too many actions waiting";
        g_race_control.push_back(a);
        return {};
    }

    void set_auto_penalties(bool enabled) override
    {
        g_auto = enabled;
        swofty::log("[steward] automatic penalties %s", enabled ? "on" : "off");
        local_event("LOCAL", std::string("Steward: automatic penalties ") + (enabled ? "on" : "off"));
    }
};

Panel g_panel;
bool g_started = false;

void start_stewarding()
{
    if (!g_started) {
        g_started = true;
        g_start_ms = now_ms();
        telemetry_start(kTelemetryPort);
        if (!server_start(g_panel, std::span<const Asset>(kWebAssets))) {
            swofty::log("[steward] could not start the local page server");
            return;
        }
        swofty::log("[steward] page server on 127.0.0.1:%u", unsigned(server_port()));
    }
    if (server_running() && g_browser_stale.exchange(false)) {
        std::string url = "http://127.0.0.1:" + std::to_string(server_port()) + "/?t=" + server_token();
        bool ok = open_browser(url);
        swofty::log("[steward] browser %s", ok ? "opened" : "could not be opened");
    }
}

void on_session_start(const SwoftyEvent *, void *)
{
    g_session_seen = true;
    g_browser_stale = true;
}

}

SWOFTY_ON_LOAD
{
    g_start_ms = now_ms();
    auto r = swofty::events::subscribe(SWOFTY_EVENT_SESSION_START, on_session_start);
    if (r != swofty::events::Result::ok)
        swofty::log("[steward] session events are %s; the page opens once per game start", swofty::events::name(r));
}

SWOFTY_WRAP(Game_Network_SetScenario, void, void *session, uint8_t stat_id, const void *spec)
{
    orig(session, stat_id, spec);
    int role = rd<int>(reinterpret_cast<uintptr_t>(session) + kSessionRole, -1);
    g_role = role;
    if (role == 1) {
        if (g_session_seen.exchange(false))
            g_browser_stale = true;
        start_stewarding();
    }
}

SWOFTY_WRAP(Sess_CornerCutting_Update, void, void *rules)
{
    g_rules = reinterpret_cast<uintptr_t>(rules);
    g_rules_ms = now_ms();
    if (is_host())
        drain_penalties(rules);
    bool was = t_in_auto;
    t_in_auto = true;
    orig(rules);
    t_in_auto = was;
}

SWOFTY_WRAP(Sess_Issue_StopGo, void, void *rules, uint8_t *incident, uint32_t reason)
{
    if (!muted())
        orig(rules, incident, reason);
}

SWOFTY_WRAP(Sess_Issue_Warning, void, void *rules, uint8_t *incident, uint32_t reason)
{
    if (!muted())
        orig(rules, incident, reason);
}

SWOFTY_WRAP(Sess_Issue_TimePenalty, void, void *rules, uint8_t *incident, uint32_t reason, uint32_t seconds)
{
    if (!muted())
        orig(rules, incident, reason, seconds);
}

SWOFTY_WRAP(Sess_Issue_DriveThrough, void, void *rules, uint8_t *incident, uint32_t reason)
{
    if (!muted())
        orig(rules, incident, reason);
}

SWOFTY_WRAP(Sess_Issue_GridPlacePenalty, void, void *rules, uint8_t *incident, uint32_t reason, uint32_t places)
{
    if (!muted())
        orig(rules, incident, reason, places);
}

SWOFTY_WRAP(Sess_Issue_Disqualify, void, void *rules, uint8_t *incident, uint32_t reason)
{
    if (!muted())
        orig(rules, incident, reason);
}

SWOFTY_WRAP(Sess_SafetyCar_VehicleTransition, void, void *logic, void *time_info)
{
    orig(logic, time_info);
    auto l = reinterpret_cast<uintptr_t>(logic);
    int role = rd<int>(l + kLogicRole, -1);
    if (role == 1 && g_role.load() < 0) {
        g_role = 1;
        start_stewarding();
    }
    std::vector<uintptr_t> entries = logic_entries(l);
    int idx = entries.empty() ? -1 : int(rd<uint8_t>(entries.front() + kEntryState, 0xff));
    g_rc_state = idx == 0xff ? -1 : idx;
    if (role != 1 || !is_host())
        return;
    std::deque<RcAction> todo;
    {
        std::lock_guard<std::mutex> g(g_mu);
        todo.swap(g_race_control);
    }
    for (RcAction a : todo)
        run_race_control(l, a);
}

SWOFTY_ADDON()
