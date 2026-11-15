#include "ghost_rearm_logic.hpp"

#include <cstdio>
#include <cstring>
#include <initializer_list>

namespace gr = runtime::ghost_rearm;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static gr::Facts facts(bool ours, uint32_t lap3_count, int64_t waited_ms)
{
    gr::Facts f;
    f.enabled = f.laps = f.time_trial = f.lap3_bound = true;
    f.ours = ours;
    f.lap3_count = lap3_count;
    f.waited_ms = waited_ms;
    return f;
}

int main()
{
    expect(gr::decide(facts(false, 0, 5)) == gr::Do::arm, "stock content, the game offline: lap 3 emptied by the reset is armed again");
    expect(gr::decide(facts(false, 0, 900)) == gr::Do::arm, "stock content: lap 3 still empty after the settle time is armed again");
    expect(gr::decide(facts(true, 0, 5)) == gr::Do::arm, "addon content: lap 3 emptied by the reset is armed again");
    expect(gr::decide(facts(true, 1472, 700)) == gr::Do::arm, "addon content: armed after the settle time over the reloaded lap, as before");
    expect(gr::decide(facts(false, 1472, 100)) == gr::Do::wait, "stock content: waits for the game's reset");
    expect(gr::decide(facts(true, 1472, 100)) == gr::Do::wait, "addon content: waits for the game's reset");
    expect(gr::decide(facts(false, 1472, 700)) == gr::Do::game_pb, "stock content: a personal best the game kept or restored itself is left alone");

    gr::Facts f = facts(false, 0, 5);
    f.time_trial = false;
    expect(gr::decide(f) == gr::Do::not_time_trial, "not a time trial: no re-arm on stock content");
    f = facts(true, 0, 5);
    f.time_trial = false;
    expect(gr::decide(f) == gr::Do::not_time_trial, "not a time trial: no re-arm on addon content");
    f = facts(false, 0, 5);
    f.lap3_bound = false;
    expect(gr::decide(f) == gr::Do::unbound, "lap 3 not bound to a car: no re-arm");
    f = facts(true, 0, 5);
    f.enabled = false;
    expect(gr::decide(f) == gr::Do::off, "re-arm off: no re-arm");
    f = facts(true, 0, 5);
    f.laps = false;
    expect(gr::decide(f) == gr::Do::no_laps, "laps missing: no re-arm");
    expect(gr::decide(facts(false, 0, 30001)) == gr::Do::late, "more than 30 s after the lap: dropped");
    expect(gr::decide(facts(false, 0, 30000)) == gr::Do::arm, "30 s after the lap: still armed");

    for (gr::Do d : {gr::Do::off, gr::Do::no_laps, gr::Do::not_time_trial, gr::Do::late, gr::Do::game_pb, gr::Do::unbound})
        expect(std::strlen(gr::why(d)) > 0, "every cause that drops a re-arm has a log text");

    int seen = 0, lines = 0;
    for (int i = 0; i < 50; i++)
        lines += gr::log_due(seen) ? 1 : 0;
    expect(lines == gr::LOG_PER_CAUSE, "a cause is logged a bounded number of times");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
