// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - pit stops at normal crew speed with the extra 1990 time in the slow phases (its own DLL).
//
// The game plays a stop's crew sequence between its Start and Stop events at one rate, the sequence's base time
// over the stop's target time (PitStopInstance+0x3bc, set on Start, back to 1 on Stop), so a 9 s 1990 stop is the
// 2.58 s stock stop in slow motion. On a 1990 weekend, Sess_PitStopUpdateJob_Update is wrapped: once the game has
// set the rate for a sequence listed in data/pit-phases.json (tools/pit_phases.py), every update plays the
// sequence at rate 1 except for holds at the wheel-gun-on frames (70 % of the extra time, shared equally) and a
// slower stretch from just before the jack drop to the end of the work (the rest). The stop ends at the same time
// as the game's own. Setting pit_phases_1990 off, a sequence without a row, or another weekend keeps the game's
// rate.

#include <windows.h>

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
    float elapsed = 0;
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
    void **holder = swofty::game::g_Ne_SequenceNameTable.get();
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
    auto row = g_table.find(pit::key(s.name));
    float frame = at<float>(seq, 0x5c);
    if (row == g_table.end()) {
        if (g_logged++ < kLogStops)
            swofty::log("[f1-1990] pit phases: sequence \"%s\" (crew speed %.3f) has no phase row; the game's speed stays",
                        s.name.c_str(), double(rate));
        return;
    }
    const pit::Phases &p = row->second;
    s.plan = pit::plan(p, rate, g_fps);
    if (!s.plan.valid || frame < p.start - 2 || frame > p.start + 12) {
        if (g_logged++ < kLogStops)
            swofty::log("[f1-1990] pit phases: sequence \"%s\" crew speed %.3f at frame %.1f (start %.0f); the game's speed "
                        "stays",
                        s.name.c_str(), double(rate), double(frame), double(p.start));
        return;
    }
    s.active = true;
    s.seq = seq;
    s.rate = rate;
    s.elapsed = 0;
    s.stock = (p.stop - p.start) / (g_fps * rate) + (p.jc > p.stop ? (p.jc - p.stop) / g_fps : 0.f);
    if (g_logged++ < kLogStops)
        swofty::log("[f1-1990] pit phases: \"%s\" crew speed %.3f -> 1.0; +%.2f s as %zu hold(s) of %.2f s at the gun-on "
                    "frames and a jack stretch from frame %.0f at %.2f; stop %.2f s as before",
                    s.name.c_str(), double(rate), double(s.plan.extra), s.plan.holds,
                    double(s.plan.holds ? s.plan.hold_left[0] : 0.f), double(s.plan.window_start),
                    double(s.plan.window_rate), double(s.stock));
}

void finish(Stop &s, float *rate, const char *why)
{
    if (rate)
        *rate = 1.f;
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
    if (!inst || !swofty::mem::readable(inst + 0x60, 0x3c0 - 0x60) || at<uint8_t>(inst, 0x64)) {
        orig(job);
        return;
    }
    {
        std::lock_guard lock(g_lock);
        load();
        auto *seq = at<uint8_t *>(inst, 0x3b0);
        auto *rate = reinterpret_cast<float *>(inst + 0x3bc);
        float dt = at<float>(job, 0x10);
        Stop &s = slot(inst);
        if (s.active && (seq != s.seq || !seq))
            finish(s, nullptr, "sequence changed");
        if (!s.active && seq && seq != s.done && *rate > 0.01f && *rate < 0.999f &&
            swofty::mem::readable(seq + 0x08, 0x58) && !g_table.empty())
            begin(s, seq, *rate);
        if (s.active) {
            float frame = at<float>(seq, 0x5c);
            s.elapsed += dt > 0 && dt < 1 ? dt : 0;
            if (frame >= s.plan.end)
                finish(s, rate, "ended");
            else
                *rate = pit::step(s.plan, frame, dt > 0 && dt < 1 ? dt : 0);
        }
    }
    orig(job);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] pit phases: %s", pit_phases_1990 ? "on" : "off (pit_phases_1990)");
}

SWOFTY_ADDON()
