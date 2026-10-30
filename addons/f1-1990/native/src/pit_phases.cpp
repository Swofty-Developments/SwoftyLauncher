// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - pit stops at normal crew speed with the extra 1990 time in the slow phases (its own DLL).
//
// The game plays a stop's crew sequence between its Start and Stop events at one rate, the sequence's base time
// over the stop's target time, so a 9 s 1990 stop is the 2.58 s stock stop in slow motion. Start writes that crew
// speed into PitStopInstance+0x33c (the base rate) and +0x3bc (this frame's rate); the update scales its dt by
// +0x3bc on entry and rewrites +0x3bc as a transition factor x +0x33c at its end; Stop puts both back to 1. On a
// 1990 weekend, Sess_PitStopUpdateJob_Update is wrapped: once the game has set the base rate for a sequence listed in
// data/pit-phases.json (tools/pit_phases.py), every update sets the base rate and this frame's rate (base x the
// factor the game last applied) so the sequence plays at rate 1 except for holds at the wheel-gun-on frames (70 %
// of the extra time, shared equally) and a slower stretch from just before the jack drop to the end of the work
// (the rest). The stop ends at the same time as the game's own. Setting pit_phases_1990 off, a sequence without a
// row, or another weekend keeps the game's rate.

#include <windows.h>

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <unordered_map>

#include <swofty/game.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#include "pit_phases.hpp"
#include "weekend.hpp"

SWOFTY_SETTING(bool, pit_phases_1990, true);

namespace {

using f1990::at;
namespace pit = f1990::pit;

constexpr char kFile[] = "data/pit-phases.json";
constexpr size_t kInstances = 32;
constexpr int kLogStops = 32;

struct Stop {
    uint8_t *inst = nullptr;
    uint8_t *seq = nullptr;
    uint8_t *done = nullptr;
    pit::Plan plan;
    bool active = false;
    float rate = 1.f;
    float factor = 1.f;
    float elapsed = 0;
    float unit = 1.f;
    float last_t = -1;
    bool held = false;
    bool flagged = false;
    int updates = 0;
    int number = 0;
    float stock = 0;
    std::string name;
};

std::mutex g_lock;
std::atomic<bool> g_1990{false};
std::unordered_map<std::string, pit::Phases> g_table;
float g_fps = 30.f;
bool g_loaded;
Stop g_stops[kInstances];
int g_logged;
int g_begun;
constexpr int kTraceStops = 3;

void load()
{
    if (g_loaded)
        return;
    g_loaded = true;
    std::string text;
    if (auto r = swofty::storage::read(kFile, text); r != swofty::storage::Result::ok) {
        swofty::log("[f1-1990] pit phases: read %s: %s; stops keep the game's crew speed", kFile, swofty::storage::name(r));
        return;
    }
    auto doc = swofty::json::parse(text);
    const auto *seqs = doc.find("sequences");
    if (const auto *fps = doc.find("fps"))
        g_fps = float(fps->as_double(30));
    if (!seqs || !seqs->if_object()) {
        swofty::log("[f1-1990] pit phases: %s has no sequences; stops keep the game's crew speed", kFile);
        return;
    }
    for (const auto &[name, row] : *seqs->if_object()) {
        pit::Phases p;
        if (const auto *v = row.find("start"))
            p.start = float(v->as_double());
        if (const auto *v = row.find("stop"))
            p.stop = float(v->as_double());
        if (const auto *v = row.find("jc"))
            p.jc = float(v->as_double());
        if (const auto *v = row.find("jack"))
            p.jack = float(v->as_double(-1));
        if (const auto *guns = row.find("guns"); guns && guns->if_array())
            for (const auto &g : *guns->if_array())
                if (p.gun_count < p.guns.size())
                    p.guns[p.gun_count++] = float(g.as_double());
        g_table.emplace(name, p);
    }
    swofty::log("[f1-1990] pit phases: %zu crew sequence(s) from %s at %.0f fps", g_table.size(), kFile, double(g_fps));
}

void *call_getter(void *obj, size_t slot)
{
    auto **vt = *static_cast<void ***>(obj);
    return vt && vt[slot] ? reinterpret_cast<void *(*)(void *)>(vt[slot])(obj) : nullptr;
}

std::string sequence_name(const uint8_t *seq)
{
    auto *res = at<const uint8_t *>(seq, 0x08);
    if (!res || !swofty::mem::readable(res + 0x38, 4))
        return {};
    int32_t index = at<int32_t>(res, 0x38);
    void **holder = swofty::game::g_Ne_NameTable.get();
    if (index < 0 || !holder || !swofty::mem::readable(holder, 8) || !*holder ||
        !swofty::mem::readable(static_cast<uint8_t *>(*holder) + 0x08, 8))
        return {};
    auto *entries = at<const uint8_t *>(*holder, 0x08);
    if (!entries || !swofty::mem::readable(entries + size_t(index) * 0x10, 8))
        return {};
    auto *name = at<const char *>(entries + size_t(index) * 0x10, 0);
    if (!name || !swofty::mem::readable(name, 1))
        return {};
    size_t n = 0;
    while (n < 200 && swofty::mem::readable(name + n, 1) && name[n])
        n++;
    return std::string(name, n);
}

Stop &slot(uint8_t *inst)
{
    Stop *free = nullptr;
    for (auto &s : g_stops) {
        if (s.inst == inst)
            return s;
        if (!s.inst && !free)
            free = &s;
    }
    Stop &s = free ? *free : g_stops[reinterpret_cast<uintptr_t>(inst) / 0x40 % kInstances];
    s = Stop{};
    s.inst = inst;
    return s;
}

void begin(Stop &s, uint8_t *seq, float rate)
{
    s.done = seq;
    s.name = sequence_name(seq);
    const pit::Phases *row = pit::find(g_table, s.name);
    float now = at<float>(seq, 0x5c);
    auto *res = at<const uint8_t *>(seq, 0x08);
    float scale = res && swofty::mem::readable(res + 0x54, 4) ? at<float>(res, 0x54) : 0.f;
    if (!row || !(scale > 0.01f && scale < 1000.f)) {
        if (g_logged++ < kLogStops)
            swofty::log("[f1-1990] pit phases: sequence \"%s\" (crew speed %.3f, scale %.3f) has no phase row or family; "
                        "the game's speed stays",
                        s.name.c_str(), double(rate), double(scale));
        return;
    }
    float unit = scale / g_fps;
    pit::Phases p = pit::in_units(*row, unit);
    s.plan = pit::plan(p, rate, scale, now, unit);
    if (!s.plan.valid) {
        if (g_logged++ < kLogStops)
            swofty::log("[f1-1990] pit phases: sequence \"%s\" crew speed %.3f at time %.3f (start %.3f, stop %.3f, scale "
                        "%.3f) has no work left to hold; the game's speed stays",
                        s.name.c_str(), double(rate), double(now), double(p.start), double(p.stop), double(scale));
        return;
    }
    s.active = true;
    s.seq = seq;
    s.rate = rate;
    s.factor = 1.f;
    s.elapsed = 0;
    s.unit = unit;
    s.last_t = now;
    s.held = false;
    s.flagged = false;
    s.updates = 0;
    s.number = ++g_begun;
    float from = std::max(now, 0.f);
    s.stock = (p.stop - from) / (scale * rate) + (p.jc > p.stop ? (p.jc - p.stop) / scale : 0.f);
    if (g_logged++ < kLogStops)
        swofty::log("[f1-1990] pit phases: \"%s\" crew speed %.3f -> 1.0 from time %.3f (start %.3f, scale %.3f, %.4f per "
                    "frame); +%.2f s as %zu hold(s) of %.2f s at the gun-on times (first %.3f) and a jack stretch from %.3f "
                    "at %.2f; stop %.2f s as before",
                    s.name.c_str(), double(rate), double(now), double(p.start), double(scale), double(unit),
                    double(s.plan.extra), s.plan.holds, double(s.plan.holds ? s.plan.hold_left[0] : 0.f),
                    double(s.plan.holds ? s.plan.hold_frame[0] : -1.f), double(s.plan.window_start),
                    double(s.plan.window_rate), double(s.stock));
}

void fall_back(Stop &s, float *base, float *rate, float from, float to)
{
    *base = s.rate;
    *rate = s.rate * s.factor;
    s.active = false;
    s.seq = nullptr;
    swofty::log("[f1-1990] pit phases: \"%s\" kept moving during a hold (time %.3f -> %.3f at rate 0); the game's crew "
                "speed %.3f for the rest of this stop and of the session",
                s.name.c_str(), double(from), double(to), double(s.rate));
}

std::atomic<bool> g_uniform{false};

void finish(Stop &s, const char *why)
{
    s.active = false;
    s.seq = nullptr;
    if (g_logged++ < kLogStops)
        swofty::log("[f1-1990] pit phases: \"%s\" %s after %.2f s (game's stop %.2f s)", s.name.c_str(), why,
                    double(s.elapsed), double(s.stock));
}

}

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    if (!component || !swofty::mem::readable(static_cast<uint8_t *>(component) + 0x270, 8))
        return;
    void *provider = at<void *>(component, 0x270);
    void *weekend = provider ? call_getter(provider, 0x98 / 8) : nullptr;
    bool on = weekend && f1990::is_1990_weekend(weekend);
    g_uniform = false;
    if (on != g_1990.exchange(on))
        swofty::log("[f1-1990] pit phases: %s", on ? "1990 weekend: stops at crew speed 1 with held phases"
                                                    : "not a 1990 weekend: the game's crew speed");
}

SWOFTY_WRAP(Sess_PitStopUpdateJob_Update, void, void *job)
{
    if (!pit_phases_1990 || !g_1990.load() || !job || !swofty::mem::readable(static_cast<uint8_t *>(job) + 0x08, 0x0c)) {
        orig(job);
        return;
    }
    auto *inst = at<uint8_t *>(job, 0x08);
    if (!inst || !swofty::mem::readable(inst + 0x60, 0x3c0 - 0x60)) {
        orig(job);
        return;
    }
    {
        std::lock_guard lock(g_lock);
        load();
        auto *seq = at<uint8_t *>(inst, 0x3b0);
        auto *base = reinterpret_cast<float *>(inst + 0x33c);
        auto *rate = reinterpret_cast<float *>(inst + 0x3bc);
        float dt = at<float>(job, 0x10);
        float step_dt = dt > 0 && dt < 1 ? dt : 0;
        Stop &s = slot(inst);
        if (s.active && at<uint8_t>(inst, 0x64) && !s.flagged) {
            s.flagged = true;
            swofty::log("[f1-1990] pit phases: \"%s\" instance +0x64 set during the stop (time %.3f)", s.name.c_str(),
                        double(at<float>(seq, 0x5c)));
        }
        if (s.active && (seq != s.seq || !seq))
            finish(s, "sequence changed");
        if (!s.active && !g_uniform.load() && !at<uint8_t>(inst, 0x64) && seq && seq != s.done && *base > 0.01f &&
            *base < 0.999f && swofty::mem::readable(seq + 0x08, 0x58) && !g_table.empty())
            begin(s, seq, *base);
        if (s.active) {
            float now = at<float>(seq, 0x5c);
            float base_in = *base, rate_in = *rate;
            if (s.held && now - s.last_t > 0.25f * s.unit) {
                fall_back(s, base, rate, s.last_t, now);
                g_uniform = true;
            } else {
                s.elapsed += step_dt;
                bool going = pit::drive(s.plan, s.factor, now, step_dt, *base, *rate);
                s.held = going && *rate == 0.f;
                s.last_t = now;
                int n = s.updates++;
                if (s.number <= kTraceStops && (n < 8 || (n % 15 == 0 && n <= 600)))
                    swofty::log("[f1-1990] pit phases: stop %d update %d: time %.3f dt %.4f base %.3f rate %.3f -> %.3f "
                                "%.3f, +0x64 %u +0x65 %u",
                                s.number, n, double(now), double(dt), double(base_in), double(rate_in), double(*base),
                                double(*rate), unsigned(at<uint8_t>(inst, 0x64)), unsigned(at<uint8_t>(inst, 0x65)));
                if (!going)
                    finish(s, "ended");
            }
        }
    }
    orig(job);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] pit phases: %s", pit_phases_1990 ? "on" : "off (pit_phases_1990)");
}

SWOFTY_ADDON()
