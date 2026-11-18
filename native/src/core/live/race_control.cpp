#include "core/live/race_control.hpp"

#include "core/hooks/core_events.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <cstdarg>
#include <cstdio>

// The race control step: after each live tick the tracker diffs the new snapshot against the last one and the
// core fires each moment through its events, on the flow thread. Every session-level moment and every car status
// change is logged, up to LINES lines a session; sectors are hot and logged once per session by the hooks table.

namespace {

namespace lv = swofty_core::live;
namespace rcl = swofty_core::live::race_control;
namespace rc = swofty::race_control;

constexpr unsigned LINES = 200;

rcl::Tracker g_tracker;
unsigned g_lines;

void line(const char *fmt, ...)
{
    if (g_lines >= LINES)
        return;
    if (++g_lines == LINES) {
        swofty::log("[race-control] %u lines this session; the rest of the session's moments fire without a line", LINES);
        return;
    }
    char buf[256];
    va_list ap;
    va_start(ap, fmt);
    std::vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    swofty::log("[race-control] %s", buf);
}

const char *yes(bool b) { return b ? "yes" : "no"; }

void report(const lv::Snapshot &s)
{
    uint64_t have = s.ses.have;
    bool status = false, sector = false;
    for (uint32_t id = 0; id < SWOFTY_LIVE_MAX_CARS; id++)
        if (s.present[id]) {
            status = status || (s.cars[id].have & SWOFTY_LIVE_HAVE_CLASS);
            sector = sector || (s.cars[id].have & SWOFTY_LIVE_HAVE_SECTOR);
        }
    swofty::log("[race-control] reading the live state: flag %s (marshal zones %s, red flag %s, classification %s), "
                "safety car %s, red flag %s, car status %s, sectors %s",
                yes(have & (SWOFTY_LIVE_SHAVE_YELLOW | SWOFTY_LIVE_SHAVE_RED_FLAG | SWOFTY_LIVE_SHAVE_ORDER)),
                yes(have & SWOFTY_LIVE_SHAVE_YELLOW), yes(have & SWOFTY_LIVE_SHAVE_RED_FLAG), yes(have & SWOFTY_LIVE_SHAVE_ORDER),
                yes(have & SWOFTY_LIVE_SHAVE_SAFETY_CAR), yes(have & SWOFTY_LIVE_SHAVE_RED_FLAG), yes(status), yes(sector));
}

void fire(rcl::Moments &m)
{
    namespace ev = swofty_core::hooks::events;
    for (auto &e : m.red_flags) {
        line("red flag %s (was %s), lap %d", rc::name(e.state), rc::name(e.previous), e.lap);
        ev::session_red_flag(e);
    }
    for (auto &e : m.safety_cars) {
        line("safety car %s (was %s), lap %d", rc::name(e.deployment), rc::name(e.previous), e.lap);
        ev::session_safety_car(e);
    }
    for (auto &e : m.flags) {
        line("flag %s, yellow sectors 0x%x (was %s, 0x%x), lap %d", rc::name(e.flag), e.yellow_sectors, rc::name(e.previous),
             e.previous_yellow_sectors, e.lap);
        int64_t flag = int64_t(e.flag), sectors = int64_t(e.yellow_sectors);
        ev::session_flag(e);
        runtime::events_fire(SWOFTY_EVENT_FLAG, flag, sectors, nullptr);
    }
    for (auto &e : m.statuses) {
        line("vehicle %u %s (was %s), lap %d", e.car, rc::name(e.status), rc::name(e.previous), e.lap);
        ev::session_car_status(e);
    }
    for (auto &e : m.sectors)
        ev::session_sector(e);
}

}

namespace runtime {

void race_control_tick(const lv::Snapshot *s)
{
    static const bool on = enabled("events", nullptr);
    if (!on)
        return;
    uint64_t before = g_tracker.sessions();
    rcl::Moments m = g_tracker.step(s);
    if (!g_tracker.primed())
        return;
    if (g_tracker.sessions() != before) {
        g_lines = 0;
        report(*s);
    }
    if (!m.empty())
        fire(m);
}

}
