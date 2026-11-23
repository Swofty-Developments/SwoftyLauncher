// Host unit test for core/runtime/off_thread.hpp: disk work for the game thread done on a thread of its own, once or
// every so often while the game thread asks, and the newest value handed over without waiting.
// native/test/run_tests.sh off_thread builds and runs it.
#include "core/runtime/off_thread.hpp"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <string>
#include <thread>

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

using Clock = std::chrono::steady_clock;

static double ms_since(Clock::time_point t) { return std::chrono::duration<double, std::milli>(Clock::now() - t).count(); }
static void nap(int ms) { std::this_thread::sleep_for(std::chrono::milliseconds(ms)); }

static void once()
{
    std::atomic<int> runs{0};
    std::atomic<bool> on_caller{false};
    const std::thread::id caller = std::this_thread::get_id();
    runtime::OffThread w(
        [&] {
            on_caller = std::this_thread::get_id() == caller;
            nap(300);
            runs++;
        },
        0, 0);
    nap(100);
    expect(runs == 0 && w.runs() == 0, "nothing runs before the first want");
    double slowest = 0;
    for (int i = 0; i < 400; i++) {
        Clock::time_point t = Clock::now();
        w.want();
        slowest = std::max(slowest, ms_since(t));
        nap(1);
    }
    nap(100);
    expect(slowest < 20, "want never waits on a slow job");
    expect(runs == 1 && w.runs() == 1, "a job with no period runs once");
    expect(!on_caller, "the job runs on a thread of its own");
}

static void periodic()
{
    auto *runs = new std::atomic<int>{0};
    {
        runtime::OffThread w([runs] { (*runs)++; }, 100, 300);
        Clock::time_point start = Clock::now();
        while (ms_since(start) < 1000) {
            w.want();
            nap(10);
        }
        int asked = *runs;
        expect(asked >= 6 && asked <= 12, "the job runs every period while the game thread asks");
        nap(900);
        int idle = *runs;
        expect(idle - asked <= 5, "it stops once the game thread stops asking");
        nap(500);
        expect(*runs == idle, "and stays stopped");
        w.want();
        nap(250);
        expect(*runs > idle, "it runs again when asked again");
    }
    nap(200);
    int gone = *runs;
    nap(500);
    expect(*runs == gone, "the thread stops when its object goes, even while it was asked for");
}

static void value()
{
    std::atomic<int> n{0};
    std::atomic<bool> stall{true};
    runtime::OffThreadValue<std::string> v(
        [&] {
            while (stall)
                nap(5);
            return "read " + std::to_string(++n);
        },
        50, 1000);
    std::string out = "untouched";
    uint64_t seen = 0;
    Clock::time_point t = Clock::now();
    bool got = v.take(out, seen);
    expect(!got && out == "untouched" && seen == 0 && ms_since(t) < 20, "no value before the first run, without waiting");
    stall = false;
    Clock::time_point start = Clock::now();
    while (!v.take(out, seen) && ms_since(start) < 2000)
        nap(1);
    expect(out.rfind("read ", 0) == 0 && seen > 0, "the first run's value is handed over");
    uint64_t first = seen;
    std::string again = out;
    expect(!v.take(again, seen) || seen > first, "the same value is not handed over twice");
    nap(200);
    expect(v.take(out, seen) && seen > first, "a newer run's value is handed over");
}

int main()
{
    once();
    periodic();
    value();
    std::printf("%s\n", failed ? "FAILED" : "all ok");
    return failed ? 1 : 0;
}
