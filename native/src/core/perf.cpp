#include "perf.hpp"

#include "blackbox.hpp"
#include "perf_logic.hpp"
#include "runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <psapi.h>
#include <tlhelp32.h>

#include <algorithm>
#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <cwctype>
#include <deque>
#include <mutex>
#include <new>
#include <unordered_map>
#include <vector>

std::string settings_addon(const void *from, std::string &dll);

namespace perf {

std::atomic<bool> g_live{false};

namespace {

constexpr uint32_t MAX_PROBES = 512;
constexpr int DEPTH = 32;
constexpr size_t LOCALS = 256;
constexpr size_t HITCHES = 64;
constexpr size_t SLOW_CALLS = 32;
constexpr size_t TIMELINE = 160;
constexpr int BUCKETS = 581;
constexpr DWORD WRITE_MS = 15000;
constexpr int LINE_EVERY = 4;
constexpr double MERGE_MS = 100;

using U64 = std::atomic<uint64_t>;
constexpr auto rlx = std::memory_order_relaxed;

uint64_t tsc()
{
    return __builtin_ia32_rdtsc();
}

void add(U64 &a, uint64_t v)
{
    a.store(a.load(rlx) + v, rlx);
}

void raise(U64 &a, uint64_t v)
{
    if (v > a.load(rlx))
        a.store(v, rlx);
}

void raise_shared(U64 &a, uint64_t v)
{
    uint64_t was = a.load(rlx);
    while (v > was && !a.compare_exchange_weak(was, v, rlx))
        ;
}

struct Local {
    U64 calls[MAX_PROBES], ticks[MAX_PROBES], self[MAX_PROBES], worst[MAX_PROBES];
    U64 minute_worst[MAX_PROBES], minute_of[MAX_PROBES];
    U64 frame_ticks[2][MAX_PROBES], frame_of[2][MAX_PROBES];
    U64 top, top_frame[2], top_frame_of[2];
    std::atomic<bool> game;
};

struct Open {
    uint32_t id;
    uint64_t t0, child, paused;
};

struct Names {
    std::string owner, what;
    bool api;
};

struct Hitch {
    double at, ms, swofty_ms;
    std::string during, top;
};

struct SlowCall {
    double at, ms;
    uint32_t probe;
    unsigned long thread;
    std::string during;
};

struct Segment {
    double at, ms;
    uint64_t frames, slow;
    double worst_ms, swofty_ms;
    std::string what, top;
};

Names names[MAX_PROBES];
std::atomic<uint32_t> probes{0};
std::mutex names_lock;
std::unordered_map<std::string, uint32_t> name_index;

Local *locals[LOCALS];
std::atomic<size_t> local_count{0};
Local overflow;
thread_local Local *mine;
thread_local Open open_[DEPTH];
thread_local int depth_;
thread_local bool background_;

LARGE_INTEGER qpc0, qpf;
uint64_t tsc0;
std::atomic<double> ticks_per_ms{0};
std::atomic<uint64_t> slow_call_ticks{0}, hitch_ticks{0}, merge_ticks{0};
double hitch_ms = 50, slow_call_ms = 8;
bool verbose;

std::atomic<uint64_t> epoch{1}, minute{1};
std::atomic<uint64_t> last_frame{0}, last_flow{0}, present_frames{0}, flow_frames{0};
std::atomic<uint64_t> last_present{0};
std::atomic<bool> present_mode{false};
constexpr double PRESENT_GAP_MS = 250;
std::atomic<uint64_t> claimed_epoch{0}, claimed_at{0};
std::atomic<bool> claiming{false};
constexpr double CLAIM_MS = 33;

SRWLOCK frame_lock = SRWLOCK_INIT;
uint32_t hist[BUCKETS], minute_hist[BUCKETS];
uint64_t frames, slow33, slow50, slow100, slow250, frames_over_1ms, frames_over_4ms, frame_ticks_max, swofty_frame_max;
uint64_t seg_frames, seg_slow, seg_worst, seg_t0;

std::mutex rings_lock;
std::deque<Hitch> hitches;
std::deque<SlowCall> slow_calls;
uint64_t hitch_count, hitch_heavy;
std::deque<Segment> timeline;
std::string seg_what = "start";
std::vector<uint64_t> seg_base;
uint64_t seg_top;

std::atomic<uint32_t> slow_logged[MAX_PROBES];
std::atomic<uint32_t> hitch_lines{0};

HANDLE wake;
std::mutex write_lock;
std::atomic<bool> flushed{false};

double ms_of(uint64_t ticks)
{
    double r = ticks_per_ms.load(rlx);
    return r > 0 ? double(ticks) / r : 0;
}

double seconds_now()
{
    return ms_of(tsc() - tsc0) / 1000.0;
}

void calibrate()
{
    LARGE_INTEGER q;
    QueryPerformanceCounter(&q);
    uint64_t t = tsc();
    double ms = double(q.QuadPart - qpc0.QuadPart) * 1000.0 / double(qpf.QuadPart);
    if (ms > 0.5)
        ticks_per_ms.store(double(t - tsc0) / ms, rlx);
    double r = ticks_per_ms.load(rlx);
    slow_call_ticks.store(uint64_t(slow_call_ms * r), rlx);
    hitch_ticks.store(uint64_t(hitch_ms * r), rlx);
    merge_ticks.store(uint64_t(MERGE_MS * r), rlx);
}

Local *local()
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

template <class F> void each_local(F fn)
{
    size_t n = std::min(local_count.load(), LOCALS);
    for (size_t i = 0; i < n; i++)
        if (Local *l = locals[i])
            fn(*l);
    fn(overflow);
}

std::string during()
{
    std::lock_guard<std::mutex> g(rings_lock);
    return seg_what;
}

void slow_call(uint32_t id, uint64_t own)
{
    double ms = ms_of(own);
    SlowCall s{seconds_now(), ms, id, GetCurrentThreadId(), during()};
    uint32_t n = slow_logged[id - 1].fetch_add(1, rlx);
    if (n < 5 || n % 100 == 0)
        swofty::log("[perf] slow call: %s %s took %.1f ms on %sthread %lu during \"%s\"%s", names[id - 1].owner.c_str(),
                    names[id - 1].what.c_str(), ms, background_ ? "background " : "", s.thread, s.during.c_str(),
                    n == 4 ? " (further ones are logged every 100th)" : "");
    std::lock_guard<std::mutex> g(rings_lock);
    slow_calls.push_back(std::move(s));
    if (slow_calls.size() > SLOW_CALLS)
        slow_calls.pop_front();
}

void account(uint32_t id, uint64_t own, uint64_t self, bool top)
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

std::vector<uint64_t> probe_ticks(bool game_only = false, U64 (Local::*field)[MAX_PROBES] = &Local::ticks)
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

uint64_t top_ticks(int game = -1)
{
    uint64_t t = 0;
    each_local([&](Local &l) {
        if (game < 0 || l.game.load(rlx) == bool(game))
            t += l.top.load(rlx);
    });
    return t;
}

std::string top_list(const std::vector<std::pair<uint64_t, uint32_t>> &in, size_t n, double floor_ms)
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

int bucket(double ms)
{
    if (ms < 100)
        return std::max(0, int(ms * 4));
    if (ms < 1000)
        return 400 + int((ms - 100) / 10);
    if (ms < 10000)
        return 490 + int((ms - 1000) / 100);
    return BUCKETS - 1;
}

double bucket_ms(int b)
{
    if (b < 400)
        return (b + 0.5) / 4;
    if (b < 490)
        return 100 + (b - 400 + 0.5) * 10;
    if (b < BUCKETS - 1)
        return 1000 + (b - 490 + 0.5) * 100;
    return 10000;
}

double percentile(const uint32_t *h, double p)
{
    uint64_t n = 0;
    for (int b = 0; b < BUCKETS; b++)
        n += h[b];
    if (!n)
        return 0;
    uint64_t want = uint64_t(std::ceil(p * double(n))), seen = 0;
    for (int b = 0; b < BUCKETS; b++)
        if ((seen += h[b]) >= want)
            return bucket_ms(b);
    return bucket_ms(BUCKETS - 1);
}

void hitch(uint64_t ended, double ms, double swofty_ms)
{
    std::vector<std::pair<uint64_t, uint32_t>> in;
    uint32_t n = probes.load();
    std::vector<uint64_t> per(n);
    each_local([&](Local &l) {
        if (&l == &overflow || !l.game.load(rlx))
            return;
        size_t k = ended & 1;
        for (uint32_t i = 0; i < n; i++)
            if (l.frame_of[k][i].load(rlx) == ended)
                per[i] += l.frame_ticks[k][i].load(rlx);
    });
    for (uint32_t i = 0; i < n; i++)
        if (per[i])
            in.push_back({per[i], i});
    Hitch h{seconds_now(), ms, swofty_ms, during(), top_list(in, 3, 0.01)};
    bool heavy = swofty_ms >= 1.0 || swofty_ms >= ms * 0.1;
    if ((heavy || verbose) && hitch_lines.fetch_add(1, rlx) < 20)
        swofty::log("[perf] hitch: %.0f ms frame during \"%s\"; swofty %.2f ms in it%s%s%s", ms, h.during.c_str(),
                    swofty_ms, h.top.empty() ? "" : " (", h.top.c_str(), h.top.empty() ? "" : ")");
    std::lock_guard<std::mutex> g(rings_lock);
    hitch_count++;
    hitch_heavy += swofty_ms >= 5.0;
    hitches.push_back(std::move(h));
    if (hitches.size() > HITCHES)
        hitches.pop_front();
}

void frame(uint64_t now, bool present)
{
    if (!live()) {
        epoch.fetch_add(1, rlx);
        last_frame.store(now, rlx);
        return;
    }
    AcquireSRWLockExclusive(&frame_lock);
    uint64_t prev = last_frame.exchange(now, rlx);
    uint64_t ended = epoch.fetch_add(1, rlx);
    bool switched = present_mode.exchange(present, rlx) != present;
    (present ? present_frames : flow_frames).fetch_add(1, rlx);
    if (!prev || now <= prev || switched) {
        ReleaseSRWLockExclusive(&frame_lock);
        return;
    }
    uint64_t sw = 0;
    each_local([&](Local &l) {
        if (&l != &overflow && l.game.load(rlx) && l.top_frame_of[ended & 1].load(rlx) == ended)
            sw += l.top_frame[ended & 1].load(rlx);
    });
    uint64_t dt = now - prev;
    double ms = ms_of(dt), sw_ms = ms_of(sw);
    int b = bucket(ms);
    hist[b]++;
    minute_hist[b]++;
    frames++;
    seg_frames++;
    slow33 += ms > 33.4;
    slow50 += ms > 50;
    slow100 += ms > 100;
    slow250 += ms > 250;
    seg_slow += ms > 50;
    frames_over_1ms += sw_ms > 1;
    frames_over_4ms += sw_ms > 4;
    frame_ticks_max = std::max(frame_ticks_max, dt);
    seg_worst = std::max(seg_worst, dt);
    swofty_frame_max = std::max(swofty_frame_max, sw);
    ReleaseSRWLockExclusive(&frame_lock);
    uint64_t h = hitch_ticks.load(rlx);
    if (h && dt >= h)
        hitch(ended, ms, sw_ms);
}

using FrameTokenFn = int (*)(void **, const uint32_t *);
FrameTokenFn frame_token_orig;
std::atomic<uint32_t> frame_index{0};

int frame_token_hook(void **token, const uint32_t *index)
{
    uint64_t now = tsc();
    uint32_t i = index ? *index : 0, seen = frame_index.load(rlx);
    if (i > seen && frame_index.compare_exchange_strong(seen, i, rlx)) {
        last_present.store(now, rlx);
        frame(now, true);
    } else if (i + 100000 < seen) {
        frame_index.store(i, rlx);
    }
    return frame_token_orig(token, index);
}

bool hook_import(const char *dll, const char *fn, void *repl, void **orig)
{
    auto base = reinterpret_cast<uint8_t *>(GetModuleHandleW(nullptr));
    auto nt = reinterpret_cast<IMAGE_NT_HEADERS *>(base + reinterpret_cast<IMAGE_DOS_HEADER *>(base)->e_lfanew);
    auto &dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (!dir.VirtualAddress)
        return false;
    for (auto *d = reinterpret_cast<IMAGE_IMPORT_DESCRIPTOR *>(base + dir.VirtualAddress); d->Name; d++) {
        if (_stricmp(reinterpret_cast<const char *>(base + d->Name), dll) || !d->OriginalFirstThunk)
            continue;
        auto *names_ = reinterpret_cast<IMAGE_THUNK_DATA *>(base + d->OriginalFirstThunk);
        auto *iat = reinterpret_cast<IMAGE_THUNK_DATA *>(base + d->FirstThunk);
        for (; names_->u1.AddressOfData; names_++, iat++) {
            if (IMAGE_SNAP_BY_ORDINAL(names_->u1.Ordinal))
                continue;
            auto *by = reinterpret_cast<IMAGE_IMPORT_BY_NAME *>(base + names_->u1.AddressOfData);
            if (strcmp(reinterpret_cast<const char *>(by->Name), fn))
                continue;
            DWORD old;
            if (!VirtualProtect(&iat->u1.Function, sizeof(void *), PAGE_READWRITE, &old))
                return false;
            *orig = reinterpret_cast<void *>(iat->u1.Function);
            iat->u1.Function = reinterpret_cast<ULONG_PTR>(repl);
            VirtualProtect(&iat->u1.Function, sizeof(void *), old, &old);
            return true;
        }
    }
    return false;
}

std::string fixed(std::string s, size_t w)
{
    if (s.size() > w)
        s = s.substr(0, w - 1) + "~";
    return s + std::string(w - s.size() + 1, ' ');
}

std::string clock_text(double s)
{
    char b[32];
    int t = int(s);
    snprintf(b, sizeof b, "%02d:%02d:%02d", t / 3600, t / 60 % 60, t % 60);
    return b;
}

struct Totals {
    std::vector<uint64_t> calls, ticks, self, worst;
};

Totals totals()
{
    uint32_t n = probes.load();
    Totals t{std::vector<uint64_t>(n), std::vector<uint64_t>(n), std::vector<uint64_t>(n), std::vector<uint64_t>(n)};
    each_local([&](Local &l) {
        for (uint32_t i = 0; i < n; i++) {
            t.calls[i] += l.calls[i].load(rlx);
            t.ticks[i] += l.ticks[i].load(rlx);
            t.self[i] += l.self[i].load(rlx);
            t.worst[i] = std::max(t.worst[i], l.worst[i].load(rlx));
        }
    });
    return t;
}

double process_cores(double secs, double *user, double *kernel)
{
    FILETIME c, e, k, u;
    if (!GetProcessTimes(GetCurrentProcess(), &c, &e, &k, &u) || secs <= 0)
        return 0;
    auto v = [](FILETIME f) { return double((uint64_t(f.dwHighDateTime) << 32) | f.dwLowDateTime) / 1e7; };
    *user = v(u) / secs;
    *kernel = v(k) / secs;
    return *user + *kernel;
}

void line(std::string &out, const char *fmt, ...)
{
    char b[1024];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(b, sizeof b, fmt, ap);
    va_end(ap);
    out += b;
    out += "\r\n";
}

std::string report(const char *why)
{
    calibrate();
    double up = seconds_now();
    uint32_t hcopy[BUCKETS], mcopy[BUCKETS];
    uint64_t f, s33, s50, s100, s250, o1, o4, fmax, swmax, sf, ss, sw, st0;
    AcquireSRWLockShared(&frame_lock);
    memcpy(hcopy, hist, sizeof hist);
    memcpy(mcopy, minute_hist, sizeof minute_hist);
    f = frames, s33 = slow33, s50 = slow50, s100 = slow100, s250 = slow250, o1 = frames_over_1ms, o4 = frames_over_4ms;
    fmax = frame_ticks_max, swmax = swofty_frame_max, sf = seg_frames, ss = seg_slow, sw = seg_worst, st0 = seg_t0;
    ReleaseSRWLockShared(&frame_lock);
    Totals t = totals();
    uint32_t n = uint32_t(t.calls.size());
    uint64_t top = top_ticks();
    uint64_t shown = present_frames.load(), flo = flow_frames.load();
    std::string out;
    SYSTEMTIME st;
    GetLocalTime(&st);
    line(out, "Swofty Launcher performance report (format 1)");
    line(out, "Written %04u-%02u-%02u %02u:%02u:%02u (local), %.0f s after the core attached, %s", st.wYear, st.wMonth,
         st.wDay, st.wHour, st.wMinute, st.wSecond, up, why);
    line(out, "Swofty time is time spent in the launcher's core and addons on the game's threads, game code excluded.");
    line(out, "");
    line(out, "== Summary");
    line(out, "uptime: %.0f s", up);
    line(out, "frames: %llu (%llu from the game's frame index, now %u; %llu from flow-state ticks while it is not updated)",
         (unsigned long long)f, (unsigned long long)shown, frame_index.load(), (unsigned long long)flo);
    double p50 = percentile(hcopy, 0.5), p95 = percentile(hcopy, 0.95), p99 = percentile(hcopy, 0.99);
    double m50 = percentile(mcopy, 0.5);
    line(out, "fps: %.1f median, %.1f in the current minute", p50 > 0 ? 1000 / p50 : 0, m50 > 0 ? 1000 / m50 : 0);
    line(out, "frame times: p50 %.2f ms, p95 %.2f ms, p99 %.2f ms, max %.0f ms", p50, p95, p99, ms_of(fmax));
    line(out, "slow frames: %llu over 33 ms, %llu over 50 ms, %llu over 100 ms, %llu over 250 ms", (unsigned long long)s33,
         (unsigned long long)s50, (unsigned long long)s100, (unsigned long long)s250);
    uint64_t game_top = top_ticks(1), other_top = top_ticks(0);
    line(out, "swofty time: %.1f ms in total, %.3f ms/s (%.3f%% of one core); game threads %.3f ms/s, addon threads "
              "%.3f ms/s",
         ms_of(top), up > 0 ? ms_of(top) / up : 0, up > 0 ? ms_of(top) / up / 10 : 0,
         up > 0 ? ms_of(game_top) / up : 0, up > 0 ? ms_of(other_top) / up : 0);
    line(out, "swofty per frame: max %.2f ms; %llu frame(s) over 1 ms, %llu over 4 ms", ms_of(swmax),
         (unsigned long long)o1, (unsigned long long)o4);
    uint32_t worst_i = 0;
    for (uint32_t i = 1; i < n; i++)
        if (t.worst[i] > t.worst[worst_i])
            worst_i = i;
    if (n)
        line(out, "worst call: %.2f ms, %s %s", ms_of(t.worst[worst_i]), names[worst_i].owner.c_str(),
             names[worst_i].what.c_str());
    {
        std::lock_guard<std::mutex> g(rings_lock);
        line(out, "hitches: %llu frame(s) over %.0f ms; swofty took over 5 ms in %llu of them", (unsigned long long)hitch_count,
             hitch_ms, (unsigned long long)hitch_heavy);
    }
    double user = 0, kernel = 0;
    double cores = process_cores(up, &user, &kernel);
    line(out, "game process cpu: %.2f cores on average since the core attached (user %.2f, kernel %.2f)", cores, user,
         kernel);
    line(out, "");

    struct Owner {
        std::string name;
        uint64_t calls = 0, self = 0, worst = 0, busiest = 0;
        uint32_t busiest_i = 0;
    };
    std::vector<Owner> owners;
    for (uint32_t i = 0; i < n; i++) {
        if (!t.calls[i])
            continue;
        auto it = std::find_if(owners.begin(), owners.end(), [&](const Owner &o) { return o.name == names[i].owner; });
        if (it == owners.end())
            owners.push_back({names[i].owner}), it = owners.end() - 1;
        it->calls += t.calls[i];
        it->self += t.self[i];
        it->worst = std::max(it->worst, t.worst[i]);
        if (t.self[i] >= it->busiest)
            it->busiest = t.self[i], it->busiest_i = i;
    }
    std::sort(owners.begin(), owners.end(), [](const Owner &a, const Owner &b) { return a.self > b.self; });
    line(out, "== Addons");
    line(out, "%s%12s %12s %10s %10s  %s", fixed("owner", 32).c_str(), "calls", "self ms", "ms/s", "worst ms", "busiest");
    for (auto &o : owners)
        line(out, "%s%12llu %12.1f %10.3f %10.2f  %s", fixed(o.name, 32).c_str(), (unsigned long long)o.calls,
             ms_of(o.self), up > 0 ? ms_of(o.self) / up : 0, ms_of(o.worst), names[o.busiest_i].what.c_str());
    line(out, "");

    std::vector<uint32_t> order;
    for (uint32_t i = 0; i < n; i++)
        if (t.calls[i])
            order.push_back(i);
    std::sort(order.begin(), order.end(), [&](uint32_t a, uint32_t b) { return t.ticks[a] > t.ticks[b]; });
    line(out, "== Hooks");
    line(out, "%s%s%12s %12s %10s %10s %10s", fixed("owner", 28).c_str(), fixed("probe", 44).c_str(), "calls", "total ms",
         "avg us", "worst ms", "calls/s");
    for (uint32_t i : order)
        line(out, "%s%s%12llu %12.1f %10.2f %10.2f %10.1f", fixed(names[i].owner, 28).c_str(),
             fixed(names[i].what, 44).c_str(), (unsigned long long)t.calls[i], ms_of(t.ticks[i]),
             ms_of(t.ticks[i]) * 1000 / double(t.calls[i]), ms_of(t.worst[i]), up > 0 ? double(t.calls[i]) / up : 0);
    line(out, "");

    std::lock_guard<std::mutex> g(rings_lock);
    line(out, "== Hitches (last %zu of %llu, frames over %.0f ms)", hitches.size(), (unsigned long long)hitch_count, hitch_ms);
    for (auto &h : hitches)
        line(out, "%s %9.0f ms  swofty %7.2f ms  during \"%s\"%s%s", clock_text(h.at).c_str(), h.ms, h.swofty_ms,
             h.during.c_str(), h.top.empty() ? "" : "; ", h.top.c_str());
    line(out, "");
    line(out, "== Slow calls (last %zu, single calls over %.0f ms)", slow_calls.size(), slow_call_ms);
    for (auto &c : slow_calls)
        line(out, "%s %9.1f ms  %s %s on thread %lu during \"%s\"", clock_text(c.at).c_str(), c.ms,
             names[c.probe - 1].owner.c_str(), names[c.probe - 1].what.c_str(), c.thread, c.during.c_str());
    line(out, "");
    line(out, "== Timeline (last %zu; each row runs until the next one)", timeline.size());
    line(out, "%-8s %10s %8s %6s %10s %10s  %s", "at", "took ms", "frames", "slow", "worst ms", "swofty ms", "what");
    for (auto &s : timeline)
        line(out, "%s %10.0f %8llu %6llu %10.0f %10.2f  %s%s%s", clock_text(s.at).c_str(), s.ms,
             (unsigned long long)s.frames, (unsigned long long)s.slow, s.worst_ms, s.swofty_ms, s.what.c_str(),
             s.top.empty() ? "" : "  [", s.top.empty() ? "" : (s.top + "]").c_str());
    line(out, "%s %10.0f %8llu %6llu %10.0f %10s  %s (current)", clock_text(ms_of(st0 - tsc0) / 1000).c_str(),
         ms_of(tsc() - st0), (unsigned long long)sf, (unsigned long long)ss, ms_of(sw), "-", seg_what.c_str());
    return out;
}

void write_report(const char *why)
{
    std::lock_guard<std::mutex> g(write_lock);
    std::string text = report(why);
    std::wstring path = runtime::swofty_root() + L"perf.log", tmp = path + L".tmp";
    HANDLE f = CreateFileW(tmp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    DWORD w = 0;
    bool ok = f != INVALID_HANDLE_VALUE && WriteFile(f, text.data(), DWORD(text.size()), &w, nullptr) && w == text.size();
    if (f != INVALID_HANDLE_VALUE)
        CloseHandle(f);
    if (!ok || !MoveFileExW(tmp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING))
        DeleteFileW(tmp.c_str());
}

namespace vq {

constexpr int MODS = 64, SHARDS = 16, MADE = 512;
constexpr unsigned SITE_BITS = 10;
constexpr size_t SITES = size_t(1) << SITE_BITS;

struct Mod {
    uintptr_t base = 0, size = 0;
    std::string label;
    bool exe = false;
    int slots = 0;
};

struct alignas(64) Cell {
    U64 calls, ticks, game_calls, game_ticks;
};

struct Site {
    std::atomic<uintptr_t> at;
    U64 calls, ticks;
};

struct Made {
    std::atomic<DWORD> tid;
    std::atomic<uintptr_t> at;
};

using VqFn = SIZE_T(WINAPI *)(LPCVOID, PMEMORY_BASIC_INFORMATION, SIZE_T);
using ThreadFn = unsigned(__stdcall *)(void *);
using BeginFn = uintptr_t(__cdecl *)(void *, unsigned, ThreadFn, void *, unsigned, unsigned *);
using NtQitFn = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);

Mod mods[MODS];
std::atomic<int> mod_count{0};
std::atomic<uint32_t> mod_gen{0};
Cell cells[MODS + 1][SHARDS];
Site sites[SITES];
U64 site_spill_calls{0}, site_spill_ticks{0}, worst{0};
Made made[MADE];
VqFn real_vq, real_vq_base;
BeginFn real_begin;
NtQitFn nt_qit;
bool on, exe_hooked;
std::wstring root, game_dir;
thread_local uint32_t kind_gen;
thread_local bool kind_ours;

int mod_of(uintptr_t a)
{
    int n = mod_count.load(std::memory_order_acquire);
    for (int i = 0; i < n; i++)
        if (a - mods[i].base < mods[i].size)
            return i;
    return -1;
}

bool made_by_us(DWORD tid, uintptr_t *at)
{
    for (Made &m : made)
        if (m.tid.load(rlx) == tid) {
            if (at)
                *at = m.at.load(rlx);
            return true;
        }
    return false;
}

uintptr_t start_of(HANDLE thread)
{
    uintptr_t s = 0;
    if (!nt_qit || nt_qit(thread, 9, &s, sizeof s, nullptr) < 0)
        return 0;
    return s;
}

bool swofty_code(uintptr_t a)
{
    int i = mod_of(a);
    return i >= 0 && !mods[i].exe;
}

bool ours_thread()
{
    uint32_t g = mod_gen.load(rlx);
    if (kind_gen == g)
        return kind_ours;
    DWORD err = GetLastError();
    kind_ours = made_by_us(GetCurrentThreadId(), nullptr) || swofty_code(start_of(GetCurrentThread()));
    kind_gen = g;
    SetLastError(err);
    return kind_ours;
}

void count(uintptr_t ret, uint64_t dt)
{
    int i = mod_of(ret);
    bool exe = i >= 0 && mods[i].exe;
    bool game = exe || !ours_thread();
    Cell &c = cells[i < 0 ? MODS : i][(GetCurrentThreadId() >> 2) & (SHARDS - 1)];
    c.calls.fetch_add(1, rlx);
    c.ticks.fetch_add(dt, rlx);
    if (game) {
        c.game_calls.fetch_add(1, rlx);
        c.game_ticks.fetch_add(dt, rlx);
    }
    if (exe)
        return;
    raise_shared(worst, dt);
    uintptr_t site = ret;
    size_t h = logic::site_slot(site, SITE_BITS);
    for (size_t k = 0; k < 16; k++) {
        Site &s = sites[(h + k) & (SITES - 1)];
        uintptr_t at = s.at.load(rlx);
        if (!at && s.at.compare_exchange_strong(at, site, rlx))
            at = site;
        if (at != site)
            continue;
        s.calls.fetch_add(1, rlx);
        s.ticks.fetch_add(dt, rlx);
        return;
    }
    site_spill_calls.fetch_add(1, rlx);
    site_spill_ticks.fetch_add(dt, rlx);
}

SIZE_T WINAPI vq_hook(LPCVOID p, PMEMORY_BASIC_INFORMATION m, SIZE_T n)
{
    const void *ret = __builtin_return_address(0);
    uint64_t t0 = tsc();
    SIZE_T r = real_vq(p, m, n);
    uint64_t dt = tsc() - t0;
    DWORD err = GetLastError();
    count(uintptr_t(ret), dt);
    SetLastError(err);
    return r;
}

struct Start {
    ThreadFn fn;
    void *arg;
    uintptr_t at;
};

unsigned __stdcall thread_entry(void *p)
{
    Start s = *static_cast<Start *>(p);
    delete static_cast<Start *>(p);
    DWORD tid = GetCurrentThreadId();
    Made *slot = nullptr;
    for (Made &m : made) {
        DWORD free_ = 0;
        if (m.tid.load(rlx) == 0 && m.tid.compare_exchange_strong(free_, tid, rlx)) {
            m.at.store(s.at, rlx);
            slot = &m;
            break;
        }
    }
    unsigned r = s.fn(s.arg);
    if (slot) {
        slot->at.store(0, rlx);
        slot->tid.store(0, rlx);
    }
    return r;
}

uintptr_t __cdecl begin_hook(void *sec, unsigned stack, ThreadFn fn, void *arg, unsigned flags, unsigned *tid)
{
    if (!fn)
        return real_begin(sec, stack, fn, arg, flags, tid);
    void *frames[2] = {};
    USHORT n = RtlCaptureStackBackTrace(1, 2, frames, nullptr);
    uintptr_t at = n >= 2 && swofty_code(uintptr_t(frames[1])) ? uintptr_t(frames[1])
                                                                 : uintptr_t(__builtin_return_address(0));
    auto *s = new (std::nothrow) Start{fn, arg, at};
    if (!s)
        return real_begin(sec, stack, fn, arg, flags, tid);
    uintptr_t h = real_begin(sec, stack, thread_entry, s, flags, tid);
    if (!h)
        delete s;
    return h;
}

int patch(uintptr_t base, const char *fn, const void *const *match, int matches, void *repl)
{
    auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base);
    if (dos->e_magic != IMAGE_DOS_SIGNATURE)
        return 0;
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(base + dos->e_lfanew);
    if (nt->Signature != IMAGE_NT_SIGNATURE || nt->OptionalHeader.NumberOfRvaAndSizes <= IMAGE_DIRECTORY_ENTRY_IMPORT)
        return 0;
    auto &dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (!dir.VirtualAddress)
        return 0;
    int done = 0;
    for (auto *d = reinterpret_cast<const IMAGE_IMPORT_DESCRIPTOR *>(base + dir.VirtualAddress); d->Name; d++) {
        auto *iat = reinterpret_cast<IMAGE_THUNK_DATA *>(base + d->FirstThunk);
        auto *by = d->OriginalFirstThunk ? reinterpret_cast<const IMAGE_THUNK_DATA *>(base + d->OriginalFirstThunk)
                                         : nullptr;
        for (size_t k = 0; iat[k].u1.Function; k++) {
            void *now = reinterpret_cast<void *>(iat[k].u1.Function);
            bool named = !by;
            if (by && !IMAGE_SNAP_BY_ORDINAL(by[k].u1.Ordinal)) {
                auto *ibn = reinterpret_cast<const IMAGE_IMPORT_BY_NAME *>(base + by[k].u1.AddressOfData);
                named = !strcmp(reinterpret_cast<const char *>(ibn->Name), fn);
            }
            bool known = false;
            for (int j = 0; j < matches; j++)
                known |= match[j] && now == match[j];
            if (!named || !known)
                continue;
            DWORD old;
            if (!VirtualProtect(&iat[k].u1.Function, sizeof(void *), PAGE_READWRITE, &old))
                continue;
            iat[k].u1.Function = reinterpret_cast<ULONG_PTR>(repl);
            VirtualProtect(&iat[k].u1.Function, sizeof(void *), old, &old);
            done++;
        }
    }
    return done;
}

std::wstring lower(std::wstring s)
{
    for (auto &c : s)
        c = wchar_t(std::towlower(c));
    return s;
}

std::string narrow(const std::wstring &w)
{
    std::string out;
    for (wchar_t c : w)
        out += c < 128 ? char(c) : '?';
    return out;
}

bool hook(HMODULE h, const std::wstring &path, bool self)
{
    int n = mod_count.load(rlx);
    if (n >= MODS)
        return false;
    MODULEINFO mi{};
    if (!K32GetModuleInformation(GetCurrentProcess(), h, &mi, sizeof mi))
        return false;
    uintptr_t base = uintptr_t(mi.lpBaseOfDll);
    for (int i = 0; i < n; i++)
        if (mods[i].base == base)
            return false;
    bool exe = h == GetModuleHandleW(nullptr);
    std::wstring file = path.substr(path.find_last_of(L"\\/") + 1);
    Mod &m = mods[n];
    m.base = base;
    m.size = mi.SizeOfImage;
    m.exe = exe;
    m.label = self ? "core" : exe ? "game exe" : narrow(lower(file));
    mod_count.store(n + 1, std::memory_order_release);
    mod_gen.fetch_add(1, rlx);
    const void *vqs[] = {reinterpret_cast<void *>(real_vq), reinterpret_cast<void *>(real_vq_base)};
    m.slots = patch(base, "VirtualQuery", vqs, 2, reinterpret_cast<void *>(&vq_hook));
    if (!exe && real_begin) {
        const void *begins[] = {reinterpret_cast<void *>(real_begin)};
        patch(base, "_beginthreadex", begins, 1, reinterpret_cast<void *>(&begin_hook));
    }
    exe_hooked |= exe && m.slots;
    return true;
}

HMODULE self_module()
{
    HMODULE self = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&vq_hook), &self);
    return self;
}

void install()
{
    if (HMODULE nt = GetModuleHandleW(L"ntdll.dll"))
        nt_qit = reinterpret_cast<NtQitFn>(GetProcAddress(nt, "NtQueryInformationThread"));
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_PERF_VQ", v, sizeof v);
    if (n && n < sizeof v && v[0] == '0') {
        swofty::log("[perf] VirtualQuery accounting off (SWOFTY_PERF_VQ=0)");
        return;
    }
    HMODULE k32 = GetModuleHandleW(L"kernel32.dll"), kb = GetModuleHandleW(L"kernelbase.dll");
    real_vq = k32 ? reinterpret_cast<VqFn>(GetProcAddress(k32, "VirtualQuery")) : nullptr;
    real_vq_base = kb ? reinterpret_cast<VqFn>(GetProcAddress(kb, "VirtualQuery")) : nullptr;
    if (!real_vq)
        real_vq = real_vq_base;
    if (HMODULE crt = GetModuleHandleW(L"ucrtbase.dll"))
        real_begin = reinterpret_cast<BeginFn>(GetProcAddress(crt, "_beginthreadex"));
    if (!real_vq)
        return;
    on = true;
    wchar_t path[MAX_PATH * 2] = {};
    HMODULE self = self_module();
    GetModuleFileNameW(self, path, MAX_PATH * 2);
    if (hook(self, path, true))
        swofty::log("[perf] VirtualQuery accounting on in core (%d import slot(s)); thread starts %s", mods[0].slots,
                    real_begin ? "tracked through _beginthreadex" : "from start addresses only");
}

void scan()
{
    if (!on)
        return;
    if (root.empty()) {
        root = lower(runtime::swofty_root());
        size_t cut = root.find_last_of(L"\\/", root.size() >= 2 ? root.size() - 2 : 0);
        game_dir = cut == std::wstring::npos ? std::wstring() : root.substr(0, cut + 1);
    }
    HMODULE hs[1024];
    DWORD need = 0;
    if (!K32EnumProcessModules(GetCurrentProcess(), hs, sizeof hs, &need))
        return;
    HMODULE exe = GetModuleHandleW(nullptr);
    std::string added;
    for (DWORD i = 0; i < std::min<DWORD>(need / sizeof(HMODULE), 1024); i++) {
        wchar_t path[MAX_PATH * 2] = {};
        if (!GetModuleFileNameW(hs[i], path, MAX_PATH * 2))
            continue;
        std::wstring p = lower(path);
        bool ours = !root.empty() && p.rfind(root, 0) == 0;
        bool loader = !game_dir.empty() && p == game_dir + L"version.dll";
        if (hs[i] != exe && !ours && !loader)
            continue;
        if (!hook(hs[i], path, false))
            continue;
        const Mod &m = mods[mod_count.load(rlx) - 1];
        added += (added.empty() ? "" : ", ") + m.label + " (" + std::to_string(m.slots) + ")";
    }
    if (!added.empty())
        swofty::log("[perf] VirtualQuery accounting on in %s; import slot(s) in brackets", added.c_str());
}

std::string site_label(uintptr_t at)
{
    int i = mod_of(at);
    return i < 0 ? logic::rva_label("?", at) : logic::rva_label(mods[i].label, at - mods[i].base);
}

struct Last {
    uint64_t cell[MODS + 1][4] = {};
    uint64_t site[SITES][2] = {};
    uint64_t spill[2] = {};
    uint64_t page_resident = 0, page_region = 0;
};

logic::VqMinute minute(Last &last, double secs)
{
    logic::VqMinute out;
    out.secs = secs;
    out.exe_hooked = exe_hooked;
    int n = mod_count.load(std::memory_order_acquire);
    for (int i = 0; i <= MODS; i++) {
        if (i < MODS && i >= n)
            continue;
        uint64_t now[4] = {};
        for (Cell &c : cells[i]) {
            now[0] += c.calls.load(rlx);
            now[1] += c.ticks.load(rlx);
            now[2] += c.game_calls.load(rlx);
            now[3] += c.game_ticks.load(rlx);
        }
        uint64_t d[4];
        for (int k = 0; k < 4; k++)
            d[k] = logic::delta(now[k], last.cell[i][k]), last.cell[i][k] = now[k];
        if (i < MODS && mods[i].exe) {
            out.exe_calls = d[0];
            out.exe_ms = ms_of(d[1]);
            continue;
        }
        out.calls += d[0];
        out.ms += ms_of(d[1]);
        out.game_calls += d[2];
        out.game_ms += ms_of(d[3]);
        out.modules.push_back({i < MODS ? mods[i].label : "other", d[0], ms_of(d[1])});
    }
    for (size_t k = 0; k < SITES; k++) {
        uintptr_t at = sites[k].at.load(rlx);
        if (!at)
            continue;
        uint64_t c = sites[k].calls.load(rlx), t = sites[k].ticks.load(rlx);
        uint64_t dc = logic::delta(c, last.site[k][0]), dt = logic::delta(t, last.site[k][1]);
        last.site[k][0] = c, last.site[k][1] = t;
        if (dc)
            out.sites.push_back({site_label(at), dc, ms_of(dt)});
    }
    uint64_t sc = site_spill_calls.load(rlx), st = site_spill_ticks.load(rlx);
    if (uint64_t dc = logic::delta(sc, last.spill[0]))
        out.sites.push_back({"other sites", dc, ms_of(logic::delta(st, last.spill[1]))});
    last.spill[0] = sc, last.spill[1] = st;
    out.worst_ms = ms_of(worst.exchange(0, rlx));
    swofty::mem::shared::Counts pages = swofty::mem::shared::counts();
    out.page_resident = logic::delta(pages.resident, last.page_resident);
    out.page_region = logic::delta(pages.region, last.page_region);
    last.page_resident = pages.resident, last.page_region = pages.region;
    return out;
}

struct Seen {
    uint64_t created = 0, user = 0, kernel = 0;
    bool ours = false, alive = false;
    std::string what;
};

uint64_t ft(FILETIME f)
{
    return (uint64_t(f.dwHighDateTime) << 32) | f.dwLowDateTime;
}

struct Cpu {
    std::unordered_map<DWORD, Seen> seen;
    uint64_t user = 0, kernel = 0, at = 0;
    bool primed = false;
};

logic::CpuMinute cpu(Cpu &st, double secs)
{
    logic::CpuMinute out;
    out.secs = secs;
    FILETIME c, e, k, u;
    if (GetProcessTimes(GetCurrentProcess(), &c, &e, &k, &u)) {
        out.user = logic::cores(logic::delta(ft(u), st.user), secs);
        out.kernel = logic::cores(logic::delta(ft(k), st.kernel), secs);
        st.user = ft(u), st.kernel = ft(k);
    }
    FILETIME sampled;
    GetSystemTimeAsFileTime(&sampled);
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snap == INVALID_HANDLE_VALUE)
        return out;
    for (auto &[tid, s] : st.seen)
        s.alive = false;
    DWORD pid = GetCurrentProcessId();
    uint64_t ou = 0, ok = 0;
    THREADENTRY32 te{};
    te.dwSize = sizeof te;
    for (BOOL more = Thread32First(snap, &te); more; more = Thread32Next(snap, &te)) {
        if (te.th32OwnerProcessID != pid)
            continue;
        DWORD tid = te.th32ThreadID;
        HANDLE h = OpenThread(THREAD_QUERY_INFORMATION, FALSE, tid);
        if (!h)
            h = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, tid);
        if (!h)
            continue;
        if (GetThreadTimes(h, &c, &e, &k, &u)) {
            auto [it, fresh] = st.seen.try_emplace(tid);
            Seen &s = it->second;
            if (fresh || s.created != ft(c)) {
                s = Seen();
                s.created = ft(c);
                uintptr_t at = 0;
                if (made_by_us(tid, &at)) {
                    s.ours = true;
                    s.what = site_label(at) + " (tid " + std::to_string(tid) + ", via _beginthreadex)";
                } else if (uintptr_t start = start_of(h); swofty_code(start)) {
                    s.ours = true;
                    s.what = site_label(start) + " (tid " + std::to_string(tid) + ")";
                }
                if (!st.primed || s.created < st.at)
                    s.user = ft(u), s.kernel = ft(k);
            }
            s.alive = true;
            if (s.ours) {
                uint64_t du = logic::delta(ft(u), s.user), dk = logic::delta(ft(k), s.kernel);
                ou += du, ok += dk;
                out.threads++;
                out.top.push_back({s.what, logic::cores(du + dk, secs)});
            }
            s.user = ft(u), s.kernel = ft(k);
        }
        CloseHandle(h);
    }
    CloseHandle(snap);
    std::erase_if(st.seen, [](const auto &kv) { return !kv.second.alive; });
    out.ours_user = logic::cores(ou, secs);
    out.ours_kernel = logic::cores(ok, secs);
    st.primed = true;
    st.at = ft(sampled);
    return out;
}

}

struct Minute {
    uint64_t top = 0, frames = 0;
    std::vector<uint64_t> ticks, calls;
    double at = 0;
    vq::Last *vq = nullptr;
    vq::Cpu cpu;
};

void minute_line(Minute &last)
{
    double now = seconds_now();
    uint32_t mh[BUCKETS];
    uint64_t f;
    AcquireSRWLockExclusive(&frame_lock);
    memcpy(mh, minute_hist, sizeof mh);
    memset(minute_hist, 0, sizeof minute_hist);
    f = frames;
    ReleaseSRWLockExclusive(&frame_lock);
    uint64_t m = minute.fetch_add(1);
    uint64_t top = top_ticks(1);
    std::vector<uint64_t> ticks = probe_ticks(true), calls = probe_ticks(true, &Local::calls);
    double secs = now - last.at;
    uint64_t df = f - last.frames;
    uint32_t over50 = 0;
    for (int b = bucket(50.0001); b < BUCKETS; b++)
        over50 += mh[b];
    double p50 = percentile(mh, 0.5), p99 = percentile(mh, 0.99);
    std::vector<logic::Busy> in;
    for (uint32_t i = 0; i < ticks.size(); i++) {
        uint64_t was = i < last.ticks.size() ? last.ticks[i] : 0;
        uint64_t was_calls = i < last.calls.size() ? last.calls[i] : 0;
        if (ticks[i] > was)
            in.push_back({names[i].owner + " " + names[i].what, ms_of(ticks[i] - was),
                          logic::delta(i < calls.size() ? calls[i] : 0, was_calls)});
    }
    uint64_t worst = 0;
    uint32_t worst_i = 0;
    each_local([&](Local &l) {
        if (&l == &overflow)
            return;
        if (!l.game.load(rlx))
            return;
        for (uint32_t i = 0; i < ticks.size(); i++)
            if (l.minute_of[i].load(rlx) == m && l.minute_worst[i].load(rlx) > worst)
                worst = l.minute_worst[i].load(rlx), worst_i = i;
    });
    std::string busiest = logic::busiest(std::move(in), secs, 3);
    swofty::log("[perf] last %.0f s: %llu frame(s) (%.0f fps, p99 %.1f ms, %u over 50 ms); swofty %.3f ms/s on game "
                "threads%s%s%s; worst call %.2f ms%s%s%s",
                secs, (unsigned long long)df, p50 > 0 ? 1000 / p50 : 0, p99, over50,
                secs > 0 ? ms_of(top - last.top) / secs : 0, busiest.empty() ? "" : " (", busiest.c_str(),
                busiest.empty() ? "" : ")", ms_of(worst), worst ? " (" : "",
                worst ? (names[worst_i].owner + " " + names[worst_i].what).c_str() : "", worst ? ")" : "");
    if (vq::on && last.vq) {
        vq::scan();
        swofty::log("%s", logic::vq_line(vq::minute(*last.vq, secs)).c_str());
    }
    swofty::log("%s", logic::cpu_line(vq::cpu(last.cpu, secs)).c_str());
    last.top = top, last.frames = f, last.ticks = std::move(ticks), last.calls = std::move(calls), last.at = now;
    hitch_lines.store(0, rlx);
}

DWORD WINAPI writer(LPVOID)
{
    Minute last;
    vq::scan();
    last.vq = new (std::nothrow) vq::Last();
    if (last.vq)
        vq::minute(*last.vq, 0);
    vq::cpu(last.cpu, 1);
    last.at = seconds_now();
    for (unsigned tick = 1;; tick++) {
        WaitForSingleObject(wake, WRITE_MS);
        if (flushed.load())
            return 0;
        calibrate();
        if (tick % LINE_EVERY == 0)
            minute_line(last);
        write_report("periodic");
    }
}

double env_ms(const char *name, double fallback)
{
    char v[32] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    if (!n || n >= sizeof v)
        return fallback;
    double d = strtod(v, nullptr);
    return d > 0 ? d : fallback;
}

}

uint32_t probe(const std::string &owner, const std::string &what)
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

void enter(uint32_t id)
{
    int d = depth_++;
    if (d < DEPTH)
        open_[d] = {id, tsc(), 0, 0};
}

void leave()
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

void flow_tick()
{
    uint64_t now = tsc();
    uint64_t prev = last_flow.exchange(now, rlx);
    uint64_t shown = last_present.load(rlx);
    double r = ticks_per_ms.load(rlx);
    if (r <= 0 || (shown && now - shown < uint64_t(PRESENT_GAP_MS * r)))
        return;
    if (!prev || now - prev > uint64_t(2 * r))
        frame(now, false);
}

bool frame_claim()
{
    uint64_t e = epoch.load(rlx), now = tsc();
    uint64_t late = uint64_t(CLAIM_MS * ticks_per_ms.load(rlx));
    auto due = [&] { return claimed_epoch.load(rlx) != e || now - claimed_at.load(rlx) >= late; };
    if (!due() || claiming.exchange(true, std::memory_order_acquire))
        return false;
    if (!due()) {
        claiming.store(false, std::memory_order_release);
        return false;
    }
    claimed_epoch.store(e, rlx);
    claimed_at.store(now, rlx);
    return true;
}

void frame_release()
{
    claiming.store(false, std::memory_order_release);
}

void background()
{
    background_ = true;
}

void mark(const char *kind, const std::string &what)
{
    blackbox::note(kind, what.c_str());
    if (!live())
        return;
    uint64_t now = tsc();
    std::string label = std::string(kind) + " " + what;
    {
        std::lock_guard<std::mutex> g(rings_lock);
        AcquireSRWLockShared(&frame_lock);
        uint64_t t0 = seg_t0;
        ReleaseSRWLockShared(&frame_lock);
        if (now - t0 < merge_ticks.load(rlx)) {
            if (seg_what.size() < 240)
                seg_what += "; " + label;
            else if (seg_what.back() != '.')
                seg_what += "; ...";
            return;
        }
    }
    std::vector<uint64_t> base = probe_ticks(true);
    uint64_t top = top_ticks(1);
    std::lock_guard<std::mutex> g(rings_lock);
    AcquireSRWLockExclusive(&frame_lock);
    uint64_t fr = seg_frames, sl = seg_slow, worst = seg_worst, t0 = seg_t0;
    seg_frames = seg_slow = seg_worst = 0;
    seg_t0 = now;
    ReleaseSRWLockExclusive(&frame_lock);
    std::vector<std::pair<uint64_t, uint32_t>> in;
    for (uint32_t i = 0; i < base.size(); i++) {
        uint64_t was = i < seg_base.size() ? seg_base[i] : 0;
        if (base[i] > was)
            in.push_back({base[i] - was, i});
    }
    double took = ms_of(now - t0);
    timeline.push_back({ms_of(t0 - tsc0) / 1000, took, fr, sl, ms_of(worst), ms_of(top - seg_top), seg_what,
                        took >= 250 ? top_list(in, 3, 0.05) : std::string()});
    if (timeline.size() > TIMELINE)
        timeline.pop_front();
    seg_base = std::move(base);
    seg_top = top;
    seg_what = label;
}

void attach()
{
    QueryPerformanceFrequency(&qpf);
    QueryPerformanceCounter(&qpc0);
    tsc0 = tsc();
    seg_t0 = tsc0;
    LARGE_INTEGER q;
    do
        QueryPerformanceCounter(&q);
    while (double(q.QuadPart - qpc0.QuadPart) * 1000.0 / double(qpf.QuadPart) < 1.0);
    calibrate();
    bool hooked = hook_import("sl.interposer.dll", "slGetNewFrameToken", reinterpret_cast<void *>(&frame_token_hook),
                              reinterpret_cast<void **>(&frame_token_orig));
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_PROFILE", v, sizeof v);
    if (n && n < sizeof v && v[0] == '0') {
        swofty::log("[perf] profiler off (SWOFTY_PROFILE=0); frames from %s",
                    hooked ? "the game's frame index" : "the flow-state ticks");
        return;
    }
    n = GetEnvironmentVariableA("SWOFTY_PERF", v, sizeof v);
    verbose = n && n < sizeof v && v[0] && v[0] != '0';
    hitch_ms = env_ms("SWOFTY_PERF_HITCH_MS", hitch_ms);
    slow_call_ms = env_ms("SWOFTY_PERF_SLOW_CALL_MS", slow_call_ms);
    calibrate();
    g_live = true;
    vq::install();
    swofty::log("[perf] profiler on:%.0f MHz timestamp counter, hitches over %.0f ms, slow calls over %.0f ms, frames "
                "from %s; report in swofty\\perf.log every %u s",
                ticks_per_ms.load() / 1000, hitch_ms, slow_call_ms,
                hooked ? "the game's frame index (Streamline frame tokens)" : "the flow-state ticks (no Streamline import)",
                WRITE_MS / 1000);
}

void start()
{
    if (!live() || wake)
        return;
    wake = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (HANDLE t = CreateThread(nullptr, 0, writer, nullptr, 0, nullptr))
        CloseHandle(t);
}

void flush(const char *why)
{
    if (!live() || flushed.exchange(true))
        return;
    write_report(why);
    if (wake)
        SetEvent(wake);
}

int open_probes(uint32_t *out, int max)
{
    int n = std::min(std::min(depth_, DEPTH), max);
    for (int i = 0; i < n; i++)
        out[i] = open_[i].id;
    return n;
}

size_t crash_text(char *out, size_t cap, const uint32_t *open, int depth)
{
    size_t at = 0;
    auto put = [&](const char *fmt, ...) {
        if (at + 1 >= cap)
            return;
        va_list ap;
        va_start(ap, fmt);
        int n = vsnprintf(out + at, cap - at, fmt, ap);
        va_end(ap);
        at = n < 0 ? at : std::min(cap - 1, at + size_t(n));
    };
    uint32_t count = std::min(probes.load(), MAX_PROBES);
    auto name = [&](uint32_t id, const char *&owner, const char *&what) {
        owner = id && id <= count ? names[id - 1].owner.c_str() : "?";
        what = id && id <= count ? names[id - 1].what.c_str() : (id ? "?" : "game code (paused)");
    };
    if (!live()) {
        put("profiler off (SWOFTY_PROFILE=0): no probe counters\n");
        return at;
    }
    put("open on the reporting thread (outermost first):%s\n", depth ? "" : " none");
    for (int i = 0; i < depth; i++) {
        const char *o, *w;
        name(open[i], o, w);
        put("  %d. %s %s\n", i + 1, o, w);
    }
    uint64_t calls[MAX_PROBES] = {}, ticks[MAX_PROBES] = {};
    each_local([&](Local &l) {
        for (uint32_t i = 0; i < count; i++) {
            calls[i] += l.calls[i].load(rlx);
            ticks[i] += l.ticks[i].load(rlx);
        }
    });
    uint32_t order[MAX_PROBES], used = 0;
    for (uint32_t i = 0; i < count; i++)
        if (calls[i])
            order[used++] = i;
    std::sort(order, order + used, [&](uint32_t a, uint32_t b) { return calls[a] > calls[b]; });
    put("calls since the profiler started (%u of %u probe(s) called, busiest first):\n", used, count);
    for (uint32_t k = 0; k < used && k < 40; k++) {
        uint32_t i = order[k];
        put("  %12llu calls %10.1f ms  %s %s\n", (unsigned long long)calls[i], ms_of(ticks[i]), names[i].owner.c_str(),
            names[i].what.c_str());
    }
    return at;
}

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
