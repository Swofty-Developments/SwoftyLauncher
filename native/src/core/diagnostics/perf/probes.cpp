#include "core/diagnostics/perf.hpp"

#include "core/diagnostics/perf/profiler.hpp"
#include "core/diagnostics/perf_logic.hpp"
#include "core/runtime/settings.hpp"

#include <swofty/mixin.hpp>

#include <cstdio>
#include <cstring>
#include <functional>

namespace perf {

namespace detail {

Local *Profiler::local()
{
    if (mine)
        return mine;
    size_t i = local_count.fetch_add(1);
    if (i < LOCALS) {
        auto *l = new Local();
        locals[i] = l;
        mine = l;
    } else {
        local_count.store(LOCALS);
        mine = &overflow;
    }
    return mine;
}

std::string Profiler::during()
{
    std::lock_guard<std::mutex> g(rings_lock);
    return seg_what;
}

void Profiler::slow_call(uint32_t id, uint64_t own)
{
    double ms = ms_of(own);
    SlowCall s{seconds_now(), ms, id, GetCurrentThreadId(), during()};
    uint32_t n = slow_logged[id - 1].fetch_add(1, rlx);
    if (logic::slow_call_logged(n))
        swofty::log(
            "%s", logic::slow_call_line(names[id - 1].owner, names[id - 1].what, ms, background_, s.thread, s.during, n).c_str());
    std::lock_guard<std::mutex> g(rings_lock);
    slow_calls.push_back(std::move(s));
    if (slow_calls.size() > SLOW_CALLS)
        slow_calls.pop_front();
}

void Profiler::account(uint32_t id, uint64_t own, uint64_t self, bool top)
{
    Local *l = local();
    size_t i = id - 1;
    uint64_t e = epoch.load(rlx), m = minute.load(rlx);
    if (l == &overflow) {
        overflow.calls[i].fetch_add(1, rlx);
        overflow.ticks[i].fetch_add(own, rlx);
        overflow.self[i].fetch_add(self, rlx);
        raise_shared(overflow.worst[i], own);
        if (top)
            overflow.top.fetch_add(own, rlx);
    } else {
        add(l->calls[i], 1);
        add(l->ticks[i], own);
        add(l->self[i], self);
        raise(l->worst[i], own);
        if (l->minute_of[i].load(rlx) != m)
            l->minute_of[i].store(m, rlx), l->minute_worst[i].store(own, rlx);
        else
            raise(l->minute_worst[i], own);
        size_t k = e & 1;
        if (l->frame_of[k][i].load(rlx) != e)
            l->frame_of[k][i].store(e, rlx), l->frame_ticks[k][i].store(own, rlx);
        else
            add(l->frame_ticks[k][i], own);
        if (top) {
            if (!names[i].api && !background_ && !l->game.load(rlx))
                l->game.store(true, rlx);
            add(l->top, own);
            if (l->top_frame_of[k].load(rlx) != e)
                l->top_frame_of[k].store(e, rlx), l->top_frame[k].store(own, rlx);
            else
                add(l->top_frame[k], own);
        }
    }
    uint64_t slow = slow_call_ticks.load(rlx);
    if (slow && own >= slow)
        slow_call(id, own);
}

std::vector<uint64_t> Profiler::probe_ticks(bool game_only, U64 (Local::*field)[MAX_PROBES])
{
    std::vector<uint64_t> out(probes.load());
    each_local([&](Local &l) {
        if (game_only && !l.game.load(rlx))
            return;
        for (size_t i = 0; i < out.size(); i++)
            out[i] += (l.*field)[i].load(rlx);
    });
    return out;
}

uint64_t Profiler::top_ticks(int game)
{
    uint64_t t = 0;
    each_local([&](Local &l) {
        if (game < 0 || l.game.load(rlx) == bool(game))
            t += l.top.load(rlx);
    });
    return t;
}

std::string Profiler::top_list(const std::vector<std::pair<uint64_t, uint32_t>> &in, size_t n, double floor_ms)
{
    auto v = in;
    std::sort(v.begin(), v.end(), std::greater<>());
    std::string out;
    for (size_t k = 0; k < v.size() && k < n; k++) {
        double ms = ms_of(v[k].first);
        if (ms < floor_ms)
            break;
        char b[48];
        snprintf(b, sizeof b, " %.2f ms", ms);
        out += (out.empty() ? "" : ", ") + names[v[k].second].owner + " " + names[v[k].second].what + b;
    }
    return out;
}

uint32_t Profiler::probe(const std::string &owner, const std::string &what)
{
    std::string key = owner + '\n' + what;
    std::lock_guard<std::mutex> g(names_lock);
    auto it = name_index.find(key);
    if (it != name_index.end())
        return it->second;
    uint32_t n = probes.load();
    if (n == MAX_PROBES)
        return 0;
    names[n] = {owner, what, what.rfind("api ", 0) == 0};
    probes.store(n + 1);
    name_index.emplace(std::move(key), n + 1);
    return n + 1;
}

void Profiler::enter(uint32_t id)
{
    int d = depth_++;
    if (d < DEPTH)
        open_[d] = {id, tsc(), 0, 0};
}

void Profiler::leave()
{
    uint64_t t1 = tsc();
    int d = --depth_;
    if (d < 0) {
        depth_ = 0;
        return;
    }
    if (d >= DEPTH)
        return;
    Open &o = open_[d];
    uint64_t dt = t1 > o.t0 ? t1 - o.t0 : 0;
    if (!o.id) {
        if (d > 0)
            open_[d - 1].paused += dt;
        return;
    }
    uint64_t own = dt > o.paused ? dt - o.paused : 0;
    bool top = d == 0 || !open_[d - 1].id;
    if (!top)
        open_[d - 1].child += own;
    account(o.id, own, own > o.child ? own - o.child : 0, top);
}

void Profiler::background() { background_ = true; }

int Profiler::open_probes(uint32_t *out, int max)
{
    int n = std::min(std::min(depth_, DEPTH), max);
    for (int i = 0; i < n; i++)
        out[i] = open_[i].id;
    return n;
}

}

uint32_t probe(const std::string &owner, const std::string &what) { return detail::profiler.probe(owner, what); }

std::string owner_of(const void *code)
{
    std::string dll, id = settings_addon(code, dll);
    if (!id.empty())
        return id;
    HMODULE self = nullptr, m = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&owner_of), &self);
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(code), &m);
    if ((m && m == self) || !_stricmp(dll.c_str(), "swofty_gamelog.dll"))
        return "core";
    return dll.empty() ? "unknown" : dll;
}

void enter(uint32_t id) { detail::profiler.enter(id); }

void leave() { detail::profiler.leave(); }

void background() { detail::profiler.background(); }

int open_probes(uint32_t *out, int max) { return detail::profiler.open_probes(out, max); }

}

extern "C" __declspec(dllexport) uint32_t SwoftyPerfProbe(const char *owner, const char *what)
{
    std::string o = owner && *owner ? owner : perf::owner_of(__builtin_return_address(0));
    return perf::probe(o, what ? what : "?");
}

extern "C" __declspec(dllexport) void SwoftyPerfEnter(uint32_t id)
{
    if (perf::live())
        perf::enter(id);
}

extern "C" __declspec(dllexport) void SwoftyPerfLeave(void)
{
    if (perf::live())
        perf::leave();
}
