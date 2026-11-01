// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: weather (protocol/weather.md).
//
// The weekend hub generates the forecast for the eight weather datasets with
// Sess_GameWeatherSystem_SetupSession(ws, settings, config): it copies the track config into the
// TimeOfDaySystem (TOD = *(ws+0x1d98)), sets TOD+0xb426 from settings+0xb18 (exact timing) and builds
// every dataset, either from the custom slot blocks through Sess_Tod_BuildSessionDataset_Thunk or with
// the realistic generator, then flags realistic datasets in ws+0x2f3a+s.
//
// The wrap lets the game build its own forecast, then rebuilds each planned dataset through the same
// thunk with the builder's exact-timing path (the one the scripted weather handler uses): TOD+0xb426 = 1
// and per-state hold/transition seconds, so state i ends at (sum + hold_i) / len and transitions for
// trans_i / len. One state per timeline point, no generator in-between states. Unplanned datasets keep
// the game's forecast. Intensity: the builder draws each state's intensity from the per-type range
// TOD+0xb378+type*0x14 {+0xc min, +0x10 max}. A planned intensity is first clamped into that range (the
// game never draws outside it), then pins the range for the build, which is restored right after, and the
// state's own intensity is then set to the clamped value.
//
// Seed pin: GameWeatherSystem::updateRandomSeed honours the forced-seed override ws+0x1f14. The pin sets
// it and runs the game's own propagation, Sess_WeatherComponent_SetSystemRandomSeed (the component's
// rnd_seed and current weekend go to updateRandomSeed, then ws+0x1f02/+0x1f04/+0x1f08, which setupSession's
// first-weekend rule reads, are set as the game sets them). The wrap does this before the game's build.
//
// setupSession's callers: the weekend hub forecast refresh, the scripted-weather handler (settings+0xb18
// set, never touched here) and the GameWeatherSystem virtual at vtable slot +0x38 (session-time rebuild).
// A time-trial build (settings+4 == 0x12) is left alone. The build thunk does nothing while TOD+0xb432
// is set or TOD+0xb429/+0xb42a is clear, and with TOD+0xb436 set it diverts when +0xb42d or +0x2654 is
// set (V0313), so a rebuild checks the same conditions first and reports not_applied with the reason.
//
// Layouts are KB structs (re/db/structs, generated into swofty/game/session.hpp): GameWeatherSystem,
// TimeOfDayOwner, TimeOfDaySystem (clocks, datasets, lengths, type ranges, build flags), TimeOfDayClock,
// WeatherDataset, WeatherState, WeatherTypeRange, WeatherSessionSettings, WeatherAndTimeOfDayComponent.
// Every object is region-checked over sizeof(struct) before its fields are read.

#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#include "ce.hpp"
#include "components.hpp"
#include "weather_plan.hpp"

using namespace swofty::game;
using namespace ce::weather;

namespace {

constexpr int kSessions = 8;
constexpr int kNext = -1;

const char *const kKeys[kSessions] = {"p1", "p2", "p3", "q", "ss", "r", "r2", "r3"};

// Game session type of a time-trial session (an enum value, not a field).
constexpr uint32_t kSessionTimeTrial = 0x12;
constexpr uint32_t kDataCap = sizeof(WeatherDataset::states) / sizeof(WeatherState);

constexpr const char *kSidecar = "weather_plan.json";

bool readable(uintptr_t p, size_t n)
{
    if (!p || p + n < p)
        return false;
    uintptr_t at = p, end = p + n;
    while (at < end) {
        MEMORY_BASIC_INFORMATION mbi{};
        if (VirtualQuery(reinterpret_cast<void *>(at), &mbi, sizeof mbi) != sizeof mbi)
            return false;
        if (mbi.State != MEM_COMMIT || (mbi.Protect & PAGE_GUARD) || (mbi.Protect & PAGE_NOACCESS))
            return false;
        at = reinterpret_cast<uintptr_t>(mbi.BaseAddress) + mbi.RegionSize;
    }
    return true;
}

bool writable(uintptr_t p, size_t n)
{
    MEMORY_BASIC_INFORMATION mbi{};
    if (!p || VirtualQuery(reinterpret_cast<void *>(p), &mbi, sizeof mbi) != sizeof mbi)
        return false;
    if (mbi.State != MEM_COMMIT || (mbi.Protect & PAGE_GUARD))
        return false;
    DWORD w = PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    if (!(mbi.Protect & w))
        return false;
    return p + n <= reinterpret_cast<uintptr_t>(mbi.BaseAddress) + mbi.RegionSize;
}

template <class T> bool wr(uintptr_t a, T v)
{
    if (!writable(a, sizeof v))
        return false;
    std::memcpy(reinterpret_cast<void *>(a), &v, sizeof v);
    return true;
}

template <class T> bool put(T &field, T v)
{
    return wr(reinterpret_cast<uintptr_t>(&field), v);
}

GameWeatherSystem &W(uintptr_t ws)
{
    return *reinterpret_cast<GameWeatherSystem *>(ws);
}

TimeOfDaySystem &T(uintptr_t tod)
{
    return *reinterpret_cast<TimeOfDaySystem *>(tod);
}

WeatherAndTimeOfDayComponent &C(uintptr_t c)
{
    return *reinterpret_cast<WeatherAndTimeOfDayComponent *>(c);
}

// ---------------------------------------------------------------------------
// Plan sidecar (the plan model is weather_plan.cpp).

struct WeekendPlan {
    bool has[kSessions] = {};
    SessionPlan s[kSessions];
    bool empty() const
    {
        for (bool h : has)
            if (h)
                return false;
        return true;
    }
};

SRWLOCK g_lock = SRWLOCK_INIT;
struct Guard {
    Guard() { AcquireSRWLockExclusive(&g_lock); }
    ~Guard() { ReleaseSRWLockExclusive(&g_lock); }
};

bool g_loaded = false;
std::map<int, WeekendPlan> g_plans;
std::map<int, uint8_t> g_locked;
bool g_pinned = false;
uint32_t g_pin = 0;

uintptr_t g_ws = 0;
int g_weekend = -1;
uint8_t g_rebuilt = 0;
std::string g_reason[kSessions];
uint64_t g_game_sig = 0;
bool g_events = false;
double g_frame_acc = 0;
float g_last_progress = -1.0f;
int g_bind_logged = 0;

int session_of(const std::string &s)
{
    for (int i = 0; i < kSessions; ++i)
        if (s == kKeys[i])
            return i;
    return -1;
}

const char *state_name(uint32_t t)
{
    return t < 6 ? kStates[t] : "invalid";
}

void write_session(ce::Writer &w, const SessionPlan &p)
{
    w.begin_object();
    if (p.laps) {
        w.key("laps");
        w.integer(p.laps);
    }
    if (p.minutes) {
        w.key("minutes");
        w.integer(p.minutes);
    }
    if (p.start >= 0) {
        w.key("start");
        w.str(kStates[p.start]);
    }
    w.key("timeline");
    w.begin_array();
    for (const Point &pt : p.pts) {
        w.begin_object();
        w.key(kUnitKey[p.unit]);
        if (p.unit == kLap)
            w.integer(static_cast<int64_t>(pt.at));
        else
            w.num(pt.at);
        w.key("state");
        w.str(kStates[pt.state]);
        if (pt.intensity >= 0) {
            w.key("intensity");
            w.num(pt.intensity);
        }
        if (pt.ramp >= 0) {
            w.key("ramp_s");
            w.num(pt.ramp);
        }
        w.end_object();
    }
    w.end_array();
    if (p.has_air) {
        w.key("air_temp");
        w.num(p.air);
    }
    if (p.has_track) {
        w.key("track_temp");
        w.num(p.track);
    }
    w.end_object();
}

void write_weekend_sessions(ce::Writer &w, const WeekendPlan &wp)
{
    w.begin_object();
    for (int s = 0; s < kSessions; ++s)
        if (wp.has[s]) {
            w.key(kKeys[s]);
            write_session(w, wp.s[s]);
        }
    w.end_object();
}

void write_weekend_id(ce::Writer &w, int weekend)
{
    if (weekend == kNext)
        w.str("next");
    else
        w.integer(weekend);
}

struct CareerState {
    std::map<int, WeekendPlan> plans;
    std::map<int, uint8_t> locked;
    bool pinned = false;
    uint32_t pin = 0;
};

std::map<std::string, CareerState> g_careers;
std::string g_career;
int g_no_career_logged = 0;

void stash_career()
{
    if (!g_career.empty())
        g_careers[g_career] = CareerState{g_plans, g_locked, g_pinned, g_pin};
}

void write_career(ce::Writer &w, const CareerState &c)
{
    w.begin_object();
    w.key("seed");
    w.begin_object();
    w.key("pinned");
    w.boolean(c.pinned);
    w.key("value");
    w.integer(c.pin);
    w.end_object();
    w.key("plans");
    w.begin_array();
    for (auto &[wk, wp] : c.plans) {
        if (wp.empty())
            continue;
        w.begin_object();
        w.key("weekend");
        write_weekend_id(w, wk);
        w.key("sessions");
        write_weekend_sessions(w, wp);
        w.end_object();
    }
    w.end_array();
    w.key("locked");
    w.begin_array();
    for (auto &[wk, mask] : c.locked) {
        if (!mask)
            continue;
        w.begin_object();
        w.key("weekend");
        w.integer(wk);
        w.key("mask");
        w.integer(mask);
        w.end_object();
    }
    w.end_array();
    w.end_object();
}

std::string sidecar_text()
{
    stash_career();
    ce::Writer w;
    w.begin_object();
    w.key("version");
    w.integer(2);
    w.key("careers");
    w.begin_object();
    for (auto &[id, c] : g_careers) {
        if (c.plans.empty() && c.locked.empty() && !c.pinned)
            continue;
        w.key(id.c_str());
        write_career(w, c);
    }
    w.end_object();
    w.end_object();
    return w.text();
}

bool save_sidecar()
{
    auto r = swofty::storage::write(kSidecar, sidecar_text());
    if (r != swofty::storage::Result::ok) {
        ce::log("weather: sidecar %s write failed (%s)", kSidecar, swofty::storage::name(r));
        return false;
    }
    return true;
}

int weekend_of(const ce::Json &v)
{
    if (v.type == ce::Json::Str && v.s == "next")
        return kNext;
    if (is_int(v, 0, 1000))
        return static_cast<int>(v.n);
    return -2;
}

int read_career(const ce::Json &doc, CareerState &c, int &dropped)
{
    const ce::Json &seed = doc["seed"];
    if (seed.type == ce::Json::Obj) {
        c.pinned = seed["pinned"].type == ce::Json::Bool && seed["pinned"].b;
        c.pin = static_cast<uint32_t>(seed["value"].num(0));
    }
    int plans = 0;
    for (const ce::Json &e : doc["plans"].a) {
        int wk = weekend_of(e["weekend"]);
        if (wk == -2 || e["sessions"].type != ce::Json::Obj) {
            ++dropped;
            continue;
        }
        WeekendPlan &wp = c.plans[wk];
        for (auto &[k, v] : e["sessions"].o) {
            int s = session_of(k);
            SessionPlan sp;
            std::string err;
            if (s < 0 || !parse_session(v, k.c_str(), sp, err)) {
                ++dropped;
                continue;
            }
            wp.has[s] = true;
            wp.s[s] = sp;
            ++plans;
        }
    }
    for (const ce::Json &e : doc["locked"].a) {
        int wk = weekend_of(e["weekend"]);
        if (wk >= 0)
            c.locked[wk] = static_cast<uint8_t>(e["mask"].num(0));
    }
    return plans;
}

void load_sidecar()
{
    if (g_loaded)
        return;
    g_loaded = true;
    std::string text;
    auto r = swofty::storage::read(kSidecar, text);
    if (r != swofty::storage::Result::ok) {
        ce::log("weather: no sidecar %s (%s); starting with no plan", kSidecar, swofty::storage::name(r));
        return;
    }
    ce::Json doc;
    if (!json::parse(text, doc) || doc.type != ce::Json::Obj) {
        ce::log("weather: sidecar %s is not valid JSON; ignored", kSidecar);
        return;
    }
    if (doc["version"].num(0) != 2 || doc["careers"].type != ce::Json::Obj) {
        ce::log("weather: sidecar %s has no per-career plans (version %lld); ignored", kSidecar,
                doc["version"].num(0));
        return;
    }
    int plans = 0, dropped = 0;
    for (auto &[id, c] : doc["careers"].o) {
        if (id.empty() || c.type != ce::Json::Obj) {
            ++dropped;
            continue;
        }
        plans += read_career(c, g_careers[id], dropped);
    }
    ce::log("weather: sidecar loaded: %zu career(s), %d planned session(s), %d dropped", g_careers.size(), plans,
            dropped);
}

bool enter(bool sticky = false)
{
    load_sidecar();
    std::string id = ce::career_id();
    if (id.empty() && sticky && !g_career.empty())
        return true;
    if (id != g_career) {
        stash_career();
        g_career = id;
        auto it = id.empty() ? g_careers.end() : g_careers.find(id);
        CareerState c = it != g_careers.end() ? it->second : CareerState{};
        g_plans = c.plans;
        g_locked = c.locked;
        g_pinned = c.pinned;
        g_pin = c.pin;
        g_rebuilt = 0;
        for (auto &r : g_reason)
            r.clear();
        g_no_career_logged = 0;
        if (!id.empty())
            ce::log("weather: career %s: %zu weekend plan(s), seed %s %u", id.c_str(), g_plans.size(),
                    g_pinned ? "pinned" : "free", g_pin);
    }
    if (g_career.empty() && g_no_career_logged++ == 0)
        ce::log("weather: no career id; plans and the seed pin are not applied");
    return !g_career.empty();
}

// ---------------------------------------------------------------------------
// Game objects.

bool ws_ok(uintptr_t ws)
{
    return readable(ws, sizeof(GameWeatherSystem));
}

uintptr_t tod_of(uintptr_t ws)
{
    if (!ws_ok(ws))
        return 0;
    uintptr_t tod = reinterpret_cast<uintptr_t>(W(ws).tod);
    return readable(tod, sizeof(TimeOfDaySystem)) ? tod : 0;
}

uintptr_t weather_component()
{
    uintptr_t c = ce::comp::get(ce::comp::Id::WeatherAndTimeOfDay);
    return c && readable(c, sizeof(WeatherAndTimeOfDayComponent)) ? c : 0;
}

uintptr_t find_ws()
{
    if (g_ws && tod_of(g_ws))
        return g_ws;
    uintptr_t c = weather_component();
    if (!c)
        return 0;
    uintptr_t ws = reinterpret_cast<uintptr_t>(C(c).weather_system);
    return tod_of(ws) ? ws : 0;
}

WeatherDataset &dataset(uintptr_t tod, int s)
{
    return T(tod).datasets[s];
}

uint32_t state_count(uintptr_t tod, int s)
{
    uint32_t n = dataset(tod, s).state_total;
    return n > kDataCap ? kDataCap : n;
}

uint32_t transition_count(uintptr_t tod, int s)
{
    uint32_t n = state_count(tod, s), t = dataset(tod, s).state_count;
    return n == 0 ? 0 : t < n ? t : n - 1;
}

float dataset_len(uintptr_t tod, int s)
{
    return T(tod).dataset_length[s];
}

float progress(uintptr_t tod, int s)
{
    const TimeOfDayClock &c = T(tod).clocks[s];
    double a = c.time_a, b = c.time_b;
    float scale = c.time_scale;
    float len = dataset_len(tod, s);
    if (!(len > 0.001f))
        len = 0.001f;
    double r = (a + b) * scale / len;
    if (!std::isfinite(r) || r < 0)
        return 0.0f;
    return r > 1 ? 1.0f : static_cast<float>(r);
}

int current_state(uintptr_t tod, int s, float ratio)
{
    uint32_t n = state_count(tod, s), t = transition_count(tod, s);
    const WeatherState *st = dataset(tod, s).states;
    for (uint32_t i = 0; i < t; ++i) {
        float end = st[i].end, tr = st[i].transition;
        if (ratio <= end + tr)
            return static_cast<int>(i);
    }
    return n ? static_cast<int>(n) - 1 : -1;
}

std::string describe(uintptr_t tod, int s)
{
    uint32_t n = state_count(tod, s), t = transition_count(tod, s);
    const WeatherState *st = dataset(tod, s).states;
    char buf[96];
    std::string out;
    std::snprintf(buf, sizeof buf, "states=%u transitions=%u len=%.0fs:", n, t, dataset_len(tod, s));
    out = buf;
    for (uint32_t i = 0; i < n && i < 12; ++i) {
        const WeatherState &e = st[i];
        if (i < t)
            std::snprintf(buf, sizeof buf, " %s(%.2f)@%.3f/%.3f", state_name(e.type), e.intensity, e.end,
                          e.transition);
        else
            std::snprintf(buf, sizeof buf, " %s(%.2f)", state_name(e.type), e.intensity);
        out += buf;
    }
    if (n > 12)
        out += " ...";
    return out;
}

bool started(int weekend, int s)
{
    auto it = g_locked.find(weekend);
    if (it != g_locked.end() && (it->second >> s) & 1)
        return true;
    if (weekend != g_weekend || weekend < 0)
        return false;
    uintptr_t tod = tod_of(g_ws);
    return tod && T(tod).active_dataset == s && progress(tod, s) > 0.0005f;
}

int live_weekend()
{
    uintptr_t ws = find_ws();
    return ws ? static_cast<int>(W(ws).weekend) : g_weekend;
}

int default_weekend()
{
    int wk = live_weekend();
    if (wk < 0)
        return kNext;
    uintptr_t tod = wk == g_weekend ? tod_of(g_ws) : 0;
    int last_race = -1;
    for (int s = 5; tod && s < kSessions; ++s)
        if (state_count(tod, s))
            last_race = s;
    return last_race >= 0 && started(wk, last_race) ? wk + 1 : wk;
}

// ---------------------------------------------------------------------------
// Rebuild one dataset from a plan.

std::string rebuild_gate(uintptr_t ws, uintptr_t &tod, uintptr_t &owner)
{
    char buf[160];
    tod = tod_of(ws);
    if (!tod) {
        std::snprintf(buf, sizeof buf, "weather system 0x%llx has no readable TimeOfDaySystem", (unsigned long long)ws);
        return buf;
    }
    owner = reinterpret_cast<uintptr_t>(W(ws).tod_owner);
    if (!readable(owner, sizeof(TimeOfDayOwner)) ||
        reinterpret_cast<uintptr_t>(reinterpret_cast<TimeOfDayOwner *>(owner)->tod) != tod) {
        std::snprintf(buf, sizeof buf, "TOD owner 0x%llx does not hold TOD 0x%llx", (unsigned long long)owner,
                      (unsigned long long)tod);
        return buf;
    }
    if (T(tod).busy)
        return "TimeOfDaySystem busy (TOD+0xb432 set): the builder would do nothing";
    if (!T(tod).ready_a || !T(tod).ready_b)
        return "TimeOfDaySystem not ready (TOD+0xb429/+0xb42a clear): the builder would do nothing";
    if (T(tod).override_active && T(tod).random_override)
        return "TimeOfDaySystem random override (TOD+0xb436 and +0xb42d set): the build makes random states instead";
    if (T(tod).override_active && T(tod).scripted_scenario)
        return "TimeOfDaySystem scripted override (TOD+0xb436 and +0x2654 set): the build runs the scripted scenario instead";
    return "";
}

std::string rebuild(uintptr_t ws, int s, const SessionPlan &p, int weekend, const char *why)
{
    uintptr_t tod = 0, owner = 0;
    std::string gate = rebuild_gate(ws, tod, owner);
    if (!gate.empty()) {
        g_reason[s] = gate;
        ce::log("weather: not_applied %s (weekend %d, %s): %s", kKeys[s], weekend, why, gate.c_str());
        return gate;
    }
    float len = dataset_len(tod, s);
    if (!(len >= 60.0f && len <= 36000.0f))
        len = s >= 5 ? 7200.0f : 3600.0f;
    int game_first = state_count(tod, s) ? static_cast<int>(dataset(tod, s).states[0].type) : -1;
    Build b = make_build(p, len, game_first);
    if (b.dropped)
        ce::log("weather: %s (weekend %d, %s): %u point(s) dropped, no room for a %gs transition and %gs hold before the session ends",
                kKeys[s], weekend, why, b.dropped, kMinRampS, kMinHoldS);
    TypeRange ranges[6];
    for (int t = 0; t < 6; ++t)
        ranges[t] = {T(tod).type_ranges[t].intensity_min, T(tod).type_ranges[t].intensity_max};
    for (const std::string &c : clamp_build(b, ranges))
        ce::log("weather: %s (weekend %d, %s): %s", kKeys[s], weekend, why, c.c_str());

    float saved_min[6] = {}, saved_max[6] = {};
    bool pinned[6] = {};
    for (uint32_t i = 0; i < b.n; ++i) {
        uint8_t t = b.type[i];
        if (b.inten[i] < 0 || pinned[t])
            continue;
        WeatherTypeRange &r = T(tod).type_ranges[t];
        saved_min[t] = r.intensity_min;
        saved_max[t] = r.intensity_max;
        if (put(r.intensity_min, b.inten[i]) && put(r.intensity_max, b.inten[i]))
            pinned[t] = true;
    }
    uint8_t exact = T(tod).exact_timing;
    if (!put(T(tod).exact_timing, uint8_t{1})) {
        for (int t = 0; t < 6; ++t)
            if (pinned[t]) {
                WeatherTypeRange &r = T(tod).type_ranges[t];
                put(r.intensity_min, saved_min[t]);
                put(r.intensity_max, saved_max[t]);
            }
        g_reason[s] = "exact-timing flag TOD+0xb426 not writable";
        ce::log("weather: not_applied %s (weekend %d, %s): %s", kKeys[s], weekend, why, g_reason[s].c_str());
        return g_reason[s];
    }
    const uint8_t random[kMaxPlanStates] = {};
    Sess_Tod_BuildSessionDataset_Thunk(reinterpret_cast<void *>(owner), b.type, b.hold, b.trans, random, b.n, len,
                                       static_cast<uint16_t>(0x100 | s), 1);
    put(T(tod).exact_timing, exact);
    for (int t = 0; t < 6; ++t)
        if (pinned[t]) {
            WeatherTypeRange &r = T(tod).type_ranges[t];
            put(r.intensity_min, saved_min[t]);
            put(r.intensity_max, saved_max[t]);
        }
    uint32_t n = state_count(tod, s);
    WeatherState *st = dataset(tod, s).states;
    bool match = n >= b.n;
    for (uint32_t i = 0; i < b.n && match; ++i)
        match = st[i].type == b.type[i];
    if (match)
        for (uint32_t i = 0; i < b.n; ++i)
            if (b.inten[i] >= 0)
                put(st[i].intensity, b.inten[i]);
    put(W(ws).dataset_dynamic[s], uint8_t{0});
    if (match)
        g_rebuilt |= static_cast<uint8_t>(1u << s);
    else
        g_rebuilt &= static_cast<uint8_t>(~(1u << s));
    g_reason[s] = match ? "" : "MISMATCH: the game's dataset does not hold the planned states";
    ce::log("weather: rebuilt %s (weekend %d, %s) %s%s", kKeys[s], weekend, why, describe(tod, s).c_str(),
            match ? "" : " MISMATCH: the game's dataset does not hold the planned states");
    return g_reason[s];
}

int apply_weekend(uintptr_t ws, int weekend, const char *why, uint8_t only = 0xff)
{
    auto it = g_plans.find(weekend);
    if (it == g_plans.end())
        return 0;
    int done = 0;
    for (int s = 0; s < kSessions; ++s)
        if (it->second.has[s] && ((only >> s) & 1) && rebuild(ws, s, it->second.s[s], weekend, why).empty())
            ++done;
    return done;
}

// ---------------------------------------------------------------------------
// Lifecycle: lock a session when it starts, log weather progress while it runs.

void on_session_start(const SwoftyEvent *, void *)
{
    Guard g;
    if (!enter(true))
        return;
    uintptr_t tod = tod_of(g_ws);
    if (!tod || g_weekend < 0) {
        ce::log("weather: session.start with no forecast seen yet (ws 0x%llx weekend %d); nothing locked",
                (unsigned long long)g_ws, g_weekend);
        return;
    }
    uint8_t d = T(tod).active_dataset;
    if (d >= kSessions) {
        ce::log("weather: session.start active dataset %u is not a session dataset; nothing locked", d);
        return;
    }
    uint8_t was = g_locked[g_weekend];
    g_locked[g_weekend] |= static_cast<uint8_t>(1u << d);
    if (g_locked[g_weekend] != was)
        save_sidecar();
    g_last_progress = -1.0f;
    ce::log("weather: session.start dataset=%u (%s) weekend=%d -> locked; %s", d, kKeys[d], g_weekend,
            describe(tod, d).c_str());
}

void on_frame(double dt, void *)
{
    g_frame_acc += dt;
    if (g_frame_acc < 30.0)
        return;
    g_frame_acc = 0;
    Guard g;
    uintptr_t tod = tod_of(g_ws);
    if (!tod)
        return;
    uint8_t d = T(tod).active_dataset;
    if (d >= kSessions)
        return;
    float r = progress(tod, d);
    if (std::fabs(r - g_last_progress) < 0.0001f)
        return;
    g_last_progress = r;
    const TimeOfDayClock &c = T(tod).clocks[d];
    int i = current_state(tod, d, r);
    uint32_t n = state_count(tod, d);
    ce::log("weather: progress %s t=%.1fs scale=%.3f ratio=%.4f state=%d/%u %s", kKeys[d], c.time_a + c.time_b,
            c.time_scale, r, i, n, i >= 0 ? state_name(dataset(tod, d).states[i].type) : "-");
}

void ensure_events()
{
    if (g_events)
        return;
    g_events = true;
    auto a = swofty::events::subscribe(SWOFTY_EVENT_SESSION_START, on_session_start);
    auto b = swofty::events::on_frame(on_frame);
    ce::log("weather: events session.start=%s on_frame=%s", swofty::events::name(a), swofty::events::name(b));
}

std::string push_seed(uintptr_t ws, uint32_t forced, const char *why)
{
    if (!ws_ok(ws))
        return "no weather system";
    uintptr_t c = weather_component();
    if (!c || reinterpret_cast<uintptr_t>(C(c).weather_system) != ws) {
        ce::log("weather: seed not_applied (%s): no WeatherAndTimeOfDay component owns weather system 0x%llx", why,
                (unsigned long long)ws);
        return "no WeatherAndTimeOfDay component for this weather system";
    }
    uint32_t before = W(ws).base_seed;
    if (!put(W(ws).forced_seed, forced)) {
        ce::log("weather: seed not_applied (%s): forced-seed field ws+0x1f14 not writable", why);
        return "forced-seed field not writable";
    }
    Sess_WeatherComponent_SetSystemRandomSeed(reinterpret_cast<void *>(c));
    uintptr_t tod = tod_of(ws);
    ce::log("weather: seed %s %u (%s): base %u -> %u, component seed %u, weekend %u, tod seed %u, first-weekend "
            "flags %u/%u/%u",
            forced ? "pinned" : "released", forced, why, before, W(ws).base_seed, C(c).random_seed, W(ws).weekend,
            tod ? T(tod).seed : 0, W(ws).weekend_valid, W(ws).season_index, W(ws).weekend_index);
    return "";
}

void log_game_forecast(uintptr_t ws, const WeatherSessionSettings *set, bool scripted)
{
    uintptr_t tod = tod_of(ws);
    if (!tod)
        return;
    uint64_t sig = 1469598103934665603ull;
    for (int s = 0; s < kSessions; ++s) {
        uint32_t n = state_count(tod, s);
        const WeatherState *st = dataset(tod, s).states;
        for (uint32_t i = 0; i < n; ++i) {
            uint32_t words[4];
            std::memcpy(words, &st[i], sizeof words);
            for (uint32_t x : words)
                sig = (sig ^ x) * 1099511628211ull;
        }
    }
    sig ^= static_cast<uint64_t>(g_weekend) << 40;
    if (sig == g_game_sig)
        return;
    g_game_sig = sig;
    ce::log("weather: forecast weekend=%d mode=%u exact=%u%s seed=%u/%u", g_weekend,
            set ? set->mode : 0xffffffffu, set ? set->exact_timing : 0xff,
            scripted ? " (scripted, plan not applied)" : "", W(ws).base_seed, W(ws).forced_seed);
    for (int s = 0; s < kSessions; ++s)
        if (state_count(tod, s))
            ce::log("weather: game %s dynamic=%u %s", kKeys[s], W(ws).dataset_dynamic[s],
                    describe(tod, s).c_str());
}

} // namespace

// ---------------------------------------------------------------------------
// Sess_GameWeatherSystem_SetupSession: the forecast build for all eight datasets (weekend hub
// refresh and the scripted-weather handler). rcx = GameWeatherSystem, rdx = session settings.

SWOFTY_WRAP(Sess_GameWeatherSystem_SetupSession, void, void *ws, void *settings, void *config)
{
    static const uint32_t probe = ce::perf_probe("wrap Sess_GameWeatherSystem_SetupSession");
    ce::Timed timed(probe);
    ce::ensure_frame();
    uintptr_t w = reinterpret_cast<uintptr_t>(ws);
    uintptr_t set = reinterpret_cast<uintptr_t>(settings);
    const WeatherSessionSettings *sp =
        readable(set, sizeof(WeatherSessionSettings)) ? reinterpret_cast<const WeatherSessionSettings *>(set) : nullptr;
    bool scripted = sp && sp->exact_timing != 0;
    bool time_trial = sp && sp->session_type == kSessionTimeTrial;
    bool career;
    {
        Guard g;
        career = enter(true);
        ensure_events();
        if (g_bind_logged++ == 0)
            ce::log("weather: hooks bound setupSession=0x%llx build=0x%llx setSystemSeed=0x%llx",
                    (unsigned long long)Sess_GameWeatherSystem_SetupSession.address(),
                    (unsigned long long)Sess_Tod_BuildSessionDataset_Thunk.address(),
                    (unsigned long long)Sess_WeatherComponent_SetSystemRandomSeed.address());
        if (career && !scripted && !time_trial && g_pinned)
            push_seed(w, g_pin, "forecast build");
    }
    {
        ce::GameCall in_game;
        orig(ws, settings, config);
    }
    Guard g;
    if (!tod_of(w))
        return;
    int weekend = static_cast<int>(W(w).weekend);
    if (weekend != g_weekend) {
        g_rebuilt = 0;
        for (auto &r : g_reason)
            r.clear();
    }
    g_ws = w;
    g_weekend = weekend;
    bool stale = false;
    for (auto it = g_locked.begin(); career && it != g_locked.end();)
        if (it->first > weekend) {
            it = g_locked.erase(it);
            stale = true;
        } else {
            ++it;
        }
    if (stale) {
        save_sidecar();
        ce::log("weather: dropped session locks of weekends after %d (an earlier save or another career)", weekend);
    }
    log_game_forecast(w, sp, scripted);
    if (scripted || time_trial || !career)
        return;
    auto next = g_plans.find(kNext);
    if (next != g_plans.end() && !next->second.empty() && !g_plans.count(weekend)) {
        g_plans[weekend] = next->second;
        g_plans.erase(kNext);
        save_sidecar();
        ce::log("weather: plan for the next weekend bound to weekend %d", weekend);
    }
    apply_weekend(w, weekend, "forecast build");
}

// ---------------------------------------------------------------------------
// Ops.

namespace {

ce::OpResult err(const std::string &code, const std::string &detail = "")
{
    ce::OpResult r;
    r.error = detail.empty() ? code : code + ": " + detail;
    return r;
}

int target_weekend(const ce::Json &args, std::string &e)
{
    const ce::Json &v = args["weekend"];
    if (v.type == ce::Json::Null)
        return default_weekend();
    int wk = weekend_of(v);
    if (wk == -2)
        e = "weekend must be an integer 0..1000 or \"next\"";
    return wk;
}

std::string touched_json(int weekend, uint8_t mask, const std::string &extra = "")
{
    ce::Writer w;
    w.begin_object();
    w.key("weekend");
    write_weekend_id(w, weekend);
    w.key("sessions");
    w.begin_object();
    auto it = g_plans.find(weekend);
    for (int s = 0; s < kSessions; ++s) {
        if (!((mask >> s) & 1))
            continue;
        w.key(kKeys[s]);
        if (it != g_plans.end() && it->second.has[s])
            write_session(w, it->second.s[s]);
        else
            w.null();
    }
    w.end_object();
    std::string out = w.text();
    if (!extra.empty() && out.size() > 1)
        out = out.substr(0, out.size() - 1) + "," + extra + "}";
    return out;
}

std::string applied_json(uint8_t mask)
{
    std::string out = "\"applied_now\":[";
    bool first = true;
    for (int s = 0; s < kSessions; ++s)
        if ((mask >> s) & 1) {
            out += first ? "\"" : ",\"";
            out += kKeys[s];
            out += "\"";
            first = false;
        }
    return out + "]";
}

ce::OpResult op_plan_set(const ce::Json &args)
{
    Guard g;
    if (!enter())
        return err("not_ready", "no career id; weather plans are kept per career");
    std::string e;
    int weekend = target_weekend(args, e);
    if (!e.empty())
        return err("bad_args", e);
    const ce::Json &sessions = args["sessions"];
    if (sessions.type != ce::Json::Obj || sessions.o.empty())
        return err("bad_args", "sessions must be a non-empty object");
    const ce::Json &rep = args["replace"];
    if (rep.type != ce::Json::Null && rep.type != ce::Json::Bool)
        return err("bad_args", "replace must be a boolean");
    bool replace = rep.type == ce::Json::Bool && rep.b;

    WeekendPlan next = g_plans.count(weekend) ? g_plans[weekend] : WeekendPlan{};
    uint8_t touched = 0, named = 0;
    for (auto &[k, v] : sessions.o) {
        int s = session_of(k);
        if (s < 0)
            return err("bad_args", "unknown session \"" + k + "\" (p1 p2 p3 q ss r r2 r3)");
        named |= static_cast<uint8_t>(1u << s);
        if (v.type == ce::Json::Null) {
            next.has[s] = false;
            continue;
        }
        SessionPlan sp;
        if (!parse_session(v, k.c_str(), sp, e))
            return err("bad_args", e);
        next.has[s] = true;
        next.s[s] = sp;
    }
    std::string locked;
    for (int s = 0; s < kSessions; ++s) {
        bool is_locked = weekend >= 0 && started(weekend, s);
        if ((named >> s) & 1) {
            if (is_locked)
                locked += locked.empty() ? kKeys[s] : std::string(",") + kKeys[s];
            touched |= static_cast<uint8_t>(1u << s);
        } else if (replace && next.has[s] && !is_locked) {
            next.has[s] = false;
            touched |= static_cast<uint8_t>(1u << s);
        }
    }
    if (!locked.empty())
        return err("session_locked", locked);

    std::string before = touched_json(weekend, touched);
    WeekendPlan prev = g_plans.count(weekend) ? g_plans[weekend] : WeekendPlan{};
    bool existed = g_plans.count(weekend) != 0;
    g_plans[weekend] = next;
    if (!save_sidecar()) {
        if (existed)
            g_plans[weekend] = prev;
        else
            g_plans.erase(weekend);
        return err("storage", "sidecar write failed; nothing changed");
    }
    for (int s = 0; s < kSessions; ++s)
        if ((touched >> s) & 1)
            ce::log("weather.plan.set weekend=%d %s: before=%s after=%s", weekend, kKeys[s],
                    prev.has[s] ? std::to_string(prev.s[s].pts.size()).c_str() : "none",
                    next.has[s] ? (std::to_string(next.s[s].pts.size()) + " points").c_str() : "none");
    uint8_t now = 0;
    std::string skipped;
    if (weekend >= 0 && weekend == g_weekend && tod_of(g_ws))
        for (int s = 0; s < kSessions; ++s) {
            if (!((touched >> s) & 1) || !next.has[s])
                continue;
            std::string why = rebuild(g_ws, s, next.s[s], weekend, "plan.set");
            if (why.empty())
                now |= static_cast<uint8_t>(1u << s);
            else
                skipped += std::string(skipped.empty() ? "" : ",") + "\"" + kKeys[s] + "\":\"" + why + "\"";
        }
    ce::OpResult r;
    r.ok = true;
    r.before = before;
    r.after = touched_json(weekend, touched, applied_json(now) + ",\"not_applied\":{" + skipped + "}");
    return r;
}

ce::OpResult op_plan_clear(const ce::Json &args)
{
    Guard g;
    if (!enter())
        return err("not_ready", "no career id; weather plans are kept per career");
    std::string e;
    int weekend = target_weekend(args, e);
    if (!e.empty())
        return err("bad_args", e);
    uint8_t mask = 0;
    const ce::Json &list = args["sessions"];
    if (list.type == ce::Json::Null)
        mask = 0xff;
    else if (list.type != ce::Json::Arr)
        return err("bad_args", "sessions must be an array of session keys");
    else
        for (const ce::Json &k : list.a) {
            int s = k.type == ce::Json::Str ? session_of(k.s) : -1;
            if (s < 0)
                return err("bad_args", "unknown session in sessions (p1 p2 p3 q ss r r2 r3)");
            mask |= static_cast<uint8_t>(1u << s);
        }
    const ce::Json &st = args["started"];
    if (st.type != ce::Json::Null && st.type != ce::Json::Bool)
        return err("bad_args", "started must be a boolean");
    bool with_started = st.type == ce::Json::Bool && st.b;
    auto it = g_plans.find(weekend);
    uint8_t had = 0;
    if (it != g_plans.end())
        for (int s = 0; s < kSessions; ++s)
            if (it->second.has[s])
                had |= static_cast<uint8_t>(1u << s);
    mask &= had;
    uint8_t kept = 0;
    for (int s = 0; s < kSessions; ++s)
        if (!with_started && ((mask >> s) & 1) && weekend >= 0 && started(weekend, s)) {
            kept |= static_cast<uint8_t>(1u << s);
            mask &= static_cast<uint8_t>(~(1u << s));
        }
    std::string before = touched_json(weekend, mask);
    if (mask) {
        WeekendPlan prev = it->second;
        for (int s = 0; s < kSessions; ++s)
            if ((mask >> s) & 1)
                it->second.has[s] = false;
        if (!save_sidecar()) {
            it->second = prev;
            return err("storage", "sidecar write failed; nothing changed");
        }
    }
    std::string k = "\"kept\":[";
    for (int s = 0, n = 0; s < kSessions; ++s)
        if ((kept >> s) & 1)
            k += std::string(n++ ? ",\"" : "\"") + kKeys[s] + "\"";
    k += "]";
    ce::log("weather.plan.clear weekend=%d cleared=0x%02x kept=0x%02x", weekend, mask, kept);
    ce::OpResult r;
    r.ok = true;
    r.before = before;
    r.after = touched_json(weekend, mask, k);
    return r;
}

std::string seed_json(uintptr_t ws, const std::string &not_applied = "")
{
    ce::Writer w;
    w.begin_object();
    if (!not_applied.empty()) {
        w.key("not_applied");
        w.str(not_applied);
    }
    w.key("locked");
    w.boolean(g_pinned);
    w.key("seed");
    if (g_pinned)
        w.integer(g_pin);
    else if (ws_ok(ws))
        w.integer(W(ws).base_seed);
    else
        w.null();
    w.end_object();
    return w.text();
}

bool seed_arg(const ce::Json &args, uint32_t &out, std::string &e)
{
    const ce::Json &v = args["seed"];
    if (v.type == ce::Json::Null)
        return false;
    if (!is_int(v, 1, 4294967295.0)) {
        e = "seed must be an integer 1..4294967295";
        return false;
    }
    out = static_cast<uint32_t>(v.n);
    return true;
}

ce::OpResult seed_set(const ce::Json &args, bool reroll)
{
    Guard g;
    if (!enter())
        return err("not_ready", "no career id; weather plans are kept per career");
    uintptr_t ws = find_ws();
    std::string e;
    uint32_t seed = 0;
    bool given = seed_arg(args, seed, e);
    if (!e.empty())
        return err("bad_args", e);
    if (!given) {
        if (reroll) {
            LARGE_INTEGER t;
            QueryPerformanceCounter(&t);
            uint64_t x = static_cast<uint64_t>(t.QuadPart) * 6364136223846793005ull + 1442695040888963407ull;
            seed = static_cast<uint32_t>(x >> 33) | 1u;
        } else if (ws) {
            seed = W(ws).base_seed;
        } else {
            return err("not_ready", "no weather system yet; pass seed explicitly");
        }
    }
    if (!seed)
        seed = 1;
    std::string before = seed_json(ws);
    bool pinned = g_pinned;
    uint32_t pin = g_pin;
    g_pinned = true;
    g_pin = seed;
    if (!save_sidecar()) {
        g_pinned = pinned;
        g_pin = pin;
        return err("storage", "sidecar write failed; nothing changed");
    }
    std::string why = ws ? push_seed(ws, g_pin, reroll ? "seed.reroll" : "seed.lock") : "no weather system yet";
    ce::log("weather.seed.%s before=%s after=%u (%s)", reroll ? "reroll" : "lock", before.c_str(), seed,
            why.empty() ? "applied now; the forecast follows at the next hub refresh"
                        : ("stored, not_applied now: " + why + "; applies at the next forecast build").c_str());
    ce::OpResult r;
    r.ok = true;
    r.before = before;
    r.after = seed_json(ws, why);
    return r;
}

ce::OpResult op_seed_lock(const ce::Json &args)
{
    return seed_set(args, false);
}

ce::OpResult op_seed_reroll(const ce::Json &args)
{
    return seed_set(args, true);
}

ce::OpResult op_seed_unlock(const ce::Json &)
{
    Guard g;
    if (!enter())
        return err("not_ready", "no career id; weather plans are kept per career");
    uintptr_t ws = find_ws();
    std::string before = seed_json(ws);
    bool pinned = g_pinned;
    g_pinned = false;
    if (!save_sidecar()) {
        g_pinned = pinned;
        return err("storage", "sidecar write failed; nothing changed");
    }
    std::string why;
    if (ws && pinned && W(ws).forced_seed == g_pin)
        why = push_seed(ws, 0, "seed.unlock");
    else if (!ws)
        why = "no weather system yet";
    ce::log("weather.seed.unlock before=%s after=%s%s%s", before.c_str(), seed_json(ws).c_str(),
            why.empty() ? "" : " not_applied now: ", why.c_str());
    ce::OpResult r;
    r.ok = true;
    r.before = before;
    r.after = seed_json(ws, why);
    return r;
}

// ---------------------------------------------------------------------------
// Snapshot section.

void section(ce::Writer &w)
{
    Guard g;
    bool career = enter();
    ensure_events();
    uintptr_t ws = find_ws();
    uintptr_t tod = tod_of(ws);
    w.begin_object();
    w.key("available");
    w.boolean(tod != 0);
    w.key("career");
    if (career)
        w.str(g_career);
    else
        w.null();
    int live = live_weekend();
    w.key("weekend");
    if (live >= 0)
        w.integer(live);
    else
        w.null();
    w.key("forecast_weekend");
    if (g_weekend >= 0)
        w.integer(g_weekend);
    else
        w.null();
    w.key("default_weekend");
    write_weekend_id(w, default_weekend());
    w.key("seed");
    w.begin_object();
    w.key("base");
    if (ws)
        w.integer(W(ws).base_seed);
    else
        w.null();
    w.key("forced");
    if (ws)
        w.integer(W(ws).forced_seed);
    else
        w.null();
    w.key("tod");
    if (tod)
        w.integer(T(tod).seed);
    else
        w.null();
    w.key("locked");
    w.boolean(g_pinned);
    w.key("pinned");
    if (g_pinned)
        w.integer(g_pin);
    else
        w.null();
    w.end_object();
    w.key("temp_supported");
    w.boolean(false);
    w.key("active_dataset");
    if (tod)
        w.integer(T(tod).active_dataset);
    else
        w.null();
    w.key("types");
    w.begin_array();
    for (int t = 0; tod && t < 6; ++t) {
        const WeatherTypeRange &r = T(tod).type_ranges[t];
        w.begin_object();
        w.key("state");
        w.str(kStates[t]);
        w.key("min");
        w.num(r.intensity_min);
        w.key("max");
        w.num(r.intensity_max);
        w.end_object();
    }
    w.end_array();
    auto plan = g_plans.find(g_weekend >= 0 ? g_weekend : live >= 0 ? live : kNext);
    w.key("sessions");
    w.begin_object();
    for (int s = 0; tod && s < kSessions; ++s) {
        uint32_t n = state_count(tod, s);
        w.key(kKeys[s]);
        w.begin_object();
        w.key("dataset");
        w.integer(s);
        w.key("active");
        w.boolean(n != 0);
        w.key("length_s");
        w.num(dataset_len(tod, s));
        w.key("progress");
        w.num(progress(tod, s));
        bool st = g_weekend >= 0 && started(g_weekend, s);
        w.key("started");
        w.boolean(st);
        w.key("locked");
        w.boolean(st);
        w.key("planned");
        w.boolean(plan != g_plans.end() && plan->second.has[s]);
        w.key("rebuilt");
        w.boolean((g_rebuilt >> s) & 1);
        w.key("not_applied");
        if (g_reason[s].empty())
            w.null();
        else
            w.str(g_reason[s]);
        w.key("dynamic");
        w.boolean(W(ws).dataset_dynamic[s] != 0);
        w.key("states");
        w.begin_array();
        const WeatherState *states = dataset(tod, s).states;
        uint32_t walked = transition_count(tod, s);
        for (uint32_t i = 0; i < n; ++i) {
            const WeatherState &e = states[i];
            w.begin_object();
            w.key("state");
            w.str(state_name(e.type));
            w.key("intensity");
            w.num(e.intensity);
            w.key("end");
            if (i < walked)
                w.num(e.end);
            else
                w.null();
            w.key("transition");
            if (i < walked)
                w.num(e.transition);
            else
                w.null();
            w.end_object();
        }
        w.end_array();
        w.end_object();
    }
    w.end_object();
    w.key("plan");
    if (plan != g_plans.end() && !plan->second.empty()) {
        w.begin_object();
        w.key("weekend");
        write_weekend_id(w, plan->first);
        w.key("sessions");
        write_weekend_sessions(w, plan->second);
        w.end_object();
    } else {
        w.null();
    }
    w.key("plans");
    w.begin_array();
    for (auto &[wk, wp] : g_plans)
        if (!wp.empty())
            write_weekend_id(w, wk);
    w.end_array();
    w.end_object();
}

} // namespace

CE_OP_ANYTIME("weather.plan.set", op_plan_set);
CE_OP_ANYTIME("weather.plan.clear", op_plan_clear);
CE_OP_ANYTIME("weather.seed.lock", op_seed_lock);
CE_OP_ANYTIME("weather.seed.reroll", op_seed_reroll);
CE_OP_ANYTIME("weather.seed.unlock", op_seed_unlock);
CE_SECTION("weather", section);
