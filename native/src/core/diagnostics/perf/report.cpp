#include "core/diagnostics/perf.hpp"

#include "core/diagnostics/perf/profiler.hpp"
#include "core/diagnostics/perf/sysmem.hpp"
#include "core/diagnostics/perf/vq.hpp"
#include "core/diagnostics/perf_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <new>

namespace perf {

namespace detail {

std::string Profiler::fixed(std::string s, size_t w)
{
    if (s.size() > w)
        s = s.substr(0, w - 1) + "~";
    return s + std::string(w - s.size() + 1, ' ');
}

std::string Profiler::clock_text(double s)
{
    char b[32];
    int t = int(s);
    snprintf(b, sizeof b, "%02d:%02d:%02d", t / 3600, t / 60 % 60, t % 60);
    return b;
}

Profiler::Totals Profiler::totals()
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

double Profiler::process_cores(double secs, double *user, double *kernel)
{
    FILETIME c, e, k, u;
    if (!GetProcessTimes(GetCurrentProcess(), &c, &e, &k, &u) || secs <= 0)
        return 0;
    auto v = [](FILETIME f) { return double((uint64_t(f.dwHighDateTime) << 32) | f.dwLowDateTime) / 1e7; };
    *user = v(u) / secs;
    *kernel = v(k) / secs;
    return *user + *kernel;
}

void Profiler::line(std::string &out, const char *fmt, ...)
{
    char b[1024];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(b, sizeof b, fmt, ap);
    va_end(ap);
    out += b;
    out += "\r\n";
}

std::string Profiler::report(const char *why)
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
        line(out, "hitches: %llu frame(s) over %.0f ms; swofty took over 5 ms in %llu of them",
             (unsigned long long)hitch_count, hitch_ms, (unsigned long long)hitch_heavy);
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
    line(out, "== Hitches (last %zu of %llu, frames over %.0f ms)", hitches.size(), (unsigned long long)hitch_count,
         hitch_ms);
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

void Profiler::write_report(const char *why)
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

void Profiler::minute_line(Minute &last)
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
    logic::FrameMinute fm;
    fm.secs = secs, fm.frames = df, fm.p50 = p50, fm.p99 = p99, fm.over50 = over50;
    fm.swofty_ms = ms_of(top - last.top), fm.worst_ms = ms_of(worst);
    fm.busiest = logic::busiest(std::move(in), secs, 3);
    if (worst)
        fm.worst = names[worst_i].owner + " " + names[worst_i].what;
    swofty::log("%s", logic::frame_line(fm).c_str());
    if (vq::enabled() && last.vq) {
        vq::scan();
        swofty::log("%s", logic::vq_line(vq::minute(*last.vq, secs)).c_str());
    }
    swofty::log("%s", logic::cpu_line(vq::cpu(last.cpu, secs)).c_str());
    swofty::log("%s", logic::memory_line(sysmem::sample(secs)).c_str());
    last.top = top, last.frames = f, last.ticks = std::move(ticks), last.calls = std::move(calls), last.at = now;
    hitch_lines.store(0, rlx);
}

DWORD Profiler::write_loop()
{
    Minute last;
    sysmem::probe();
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

size_t Profiler::crash_text(char *out, size_t cap, const uint32_t *open, int depth)
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

DWORD WINAPI writer(LPVOID)
{
    return profiler.write_loop();
}

}

size_t crash_text(char *out, size_t cap, const uint32_t *open, int depth)
{
    return detail::profiler.crash_text(out, cap, open, depth);
}

}
