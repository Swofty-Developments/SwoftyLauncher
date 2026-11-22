#include "core/diagnostics/perf.hpp"

#include "core/diagnostics/perf/profiler.hpp"
#include "core/diagnostics/perf_logic.hpp"

#include <swofty/mixin.hpp>

#include <cstdlib>

namespace perf {

std::atomic<bool> g_live{false};

namespace detail {

Profiler profiler;

namespace {

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

void Profiler::calibrate()
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

void Profiler::attach()
{
    QueryPerformanceFrequency(&qpf);
    QueryPerformanceCounter(&qpc0);
    tsc0 = tsc();
    FILETIME now;
    GetSystemTimeAsFileTime(&now);
    wall0.LowPart = now.dwLowDateTime, wall0.HighPart = now.dwHighDateTime;
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
        swofty::log("%s", logic::profiler_off_line(hooked).c_str());
        return;
    }
    n = GetEnvironmentVariableA("SWOFTY_PERF", v, sizeof v);
    verbose = n && n < sizeof v && v[0] && v[0] != '0';
    hitch_ms = env_ms("SWOFTY_PERF_HITCH_MS", hitch_ms);
    slow_call_ms = env_ms("SWOFTY_PERF_SLOW_CALL_MS", slow_call_ms);
    calibrate();
    g_live = true;
    vq::install();
    swofty::log("%s",
                logic::profiler_on_line(ticks_per_ms.load() / 1000, hitch_ms, slow_call_ms, hooked, WRITE_MS / 1000).c_str());
}

void Profiler::start()
{
    if (!live() || wake)
        return;
    report_path();
    wake = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    DWORD id = 0;
    if (HANDLE t = CreateThread(nullptr, 0, writer, nullptr, CREATE_SUSPENDED, &id)) {
        writer_id.store(id);
        writer_thread.store(t);
        ResumeThread(t);
    }
}

void Profiler::flush(const char *why)
{
    if (!live() || flushed.exchange(true))
        return;
    write_report(why);
    if (wake)
        SetEvent(wake);
}

void Profiler::stop(DWORD wait_ms)
{
    HANDLE t = writer_thread.load();
    if (!t || GetCurrentThreadId() == writer_id.load())
        return;
    flushed.store(true);
    if (wake)
        SetEvent(wake);
    WaitForSingleObject(t, wait_ms);
}

double ms_of(uint64_t ticks) { return profiler.ms_of(ticks); }

}

void attach() { detail::profiler.attach(); }

void start() { detail::profiler.start(); }

void flush(const char *why) { detail::profiler.flush(why); }

void stop(unsigned wait_ms) { detail::profiler.stop(wait_ms); }

}
