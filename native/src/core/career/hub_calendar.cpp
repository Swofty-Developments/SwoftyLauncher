#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#include "core/apply/moments.hpp"
#include "core/career/hub_calendar_logic.hpp"

// Career hub calendar diagnostic (hub_calendar_logic.hpp). When SessionStateCareerHub enters, the core logs the
// human's calendar activities running today and what the game's calendar check makes of them. When StateSession ends
// within FAST_END_MS of entering after the hub opened, it logs "[career] session ended <N> ms after start" with the
// current weekend and the calendar again. At career.hub (Team HQ) entered it logs the current weekend, as "[career]
// weekend already started, session never ran" when the weekend is past not started with no session complete, then
// "[career] Team HQ calendar" with today's calendar items and whether they keep Team HQ's Advance off.
// career.calendar.add_activity reports each contract event or secret meeting added, with its day and today. Reads
// only, every one guarded: the ScheduleComponent (current season, season over, seasons), the season's driver
// calendars, their activities and definitions through KB structs (L2), the clock through
// CareerTimeComponent.current_minutes, and the weekend through moments::career_weekend. A build whose game headers lack
// one of those fields fails to compile.
//
// The one write, the season break fix (hub_calendar_logic.hpp): as the hub opens, a started SeasonBreak listed ahead of
// a started race weekend running today, with nothing else the check weighs before that weekend, gets
// CalendarActivity.state 4 (Ignored), one "[career] season break over race weekend" line per item, and the calendar
// is logged again. It runs on the thread that sets the hub's status, before the hub's onStateUpdate reads the calendar.
// An item whose state no longer reads 1 or whose memory is not writable is left as it is. SWOFTY_HUB_BREAK_FIX=0 turns
// it off; the line then says the break is left.
//
// The second write, the finished weekend fix (hub_calendar_logic.hpp): at the first pass of the "session already ran"
// load loop, a started race weekend that decides the hub, whose weekend is complete in every session, with a race weekend
// not started that begins inside its days, gets CalendarActivity.state 2 (Completed), one "[career] finished race weekend
// over the next one" line, and the calendar is logged again. An item whose state no longer reads 1 or whose memory is not
// writable is left as it is. SWOFTY_HUB_OVERLAP_FIX=0 turns it off; the line then says the weekend is left.
//
// At every Team HQ entry the core logs today's calendar items and whether they keep Team HQ's Advance off. When the
// only items keeping it off are ignored season breaks (hc::ended_breaks: the weekend the break ran over is done), each
// one's CalendarActivity.day moves back so the break ends the day before, one "[career] ignored season break over Team
// HQ" line per item, and the calendar is logged again. An item whose state or day no longer reads as read, or whose
// memory is not writable, is left; SWOFTY_HUB_BREAK_FIX=0 turns this off too.

namespace swofty::game {
struct ActivityDef;
struct CalendarActivity;
struct CareerTimeComponent;
struct DriverCalendar;
struct ScheduleComponent;
struct ScheduleSeason;
}

namespace {

namespace g = swofty::game;
namespace hc = runtime::hub_calendar;
namespace mem = swofty::mem;

template <class...> constexpr bool missing_layout = false;

constexpr size_t MAX_SEASONS = 64;
constexpr size_t MAX_CALENDARS = 256;

std::mutex lock;
hc::SessionWatch watch;
hc::AddLog adds;

bool read_raw(uint64_t at, void *out, size_t n, void *)
{
    if (!at || !mem::readable(uintptr_t(at), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(uintptr_t(at)), n);
    return true;
}

template <class T> T rd(uintptr_t a, T def = T{})
{
    T v{};
    return a && mem::read(a, v) ? v : def;
}

bool same_key(uint32_t a, uint32_t b) { return ((a ^ b) & 0xffff00ffu) == 0; }

struct Snapshot {
    std::string stop;
    uint32_t driver = 0, season = 0;
    uint16_t day = 0;
    bool season_over = false;
    uintptr_t calendar = 0;
    size_t total = 0;
    uint64_t state_offset = 0, day_offset = 0;
    std::vector<hc::Item> today;
};

bool player_driver(uint32_t &driver)
{
    const SwoftyCareer *career = runtime::career_table();
    SwoftyCareerState s{};
    if (!career || !career->state || career->state(&s) != SWOFTY_ASSET_OK || !s.has_player)
        return false;
    driver = s.player_driver;
    return true;
}

uintptr_t component(const char *type)
{
    const SwoftyCareer *career = runtime::career_table();
    return reinterpret_cast<uintptr_t>(career && career->component ? career->component(type) : nullptr);
}

template <class T = g::CareerTimeComponent> bool today(uint16_t &day)
{
    if constexpr (requires(T *t) { t->current_minutes; }) {
        uintptr_t time = component("CareerTimeComponent");
        uint32_t minutes = 0;
        if (!time || !mem::read(time + offsetof(T, current_minutes), minutes))
            return false;
        day = uint16_t(minutes / hc::MINUTES_PER_DAY);
        return true;
    } else {
        static_assert(missing_layout<T>, "the game headers lack CareerTimeComponent.current_minutes");
        return false;
    }
}

template <class S = g::ScheduleComponent, class Z = g::ScheduleSeason, class C = g::DriverCalendar, class A = g::CalendarActivity,
          class D = g::ActivityDef>
Snapshot snapshot()
{
    Snapshot s;
    if constexpr (requires(S *x, Z *z, C *c, A *a, D *d) {
                      x->seasons_begin;
                      x->current_season;
                      x->season_over;
                      z->driver_calendars_begin;
                      c->activities_begin;
                      c->driver;
                      a->day;
                      a->state;
                      a->activity;
                      d->id;
                      d->type;
                      d->duration;
                  }) {
        mem::Trust trust(runtime::HEAP_TRUST_MS);
        if (!player_driver(s.driver))
            return s.stop = "no single human driver", s;
        uintptr_t schedule = component("ScheduleComponent");
        if (!schedule)
            return s.stop = "no ScheduleComponent", s;
        if (!today(s.day))
            return s.stop = "the career clock does not read", s;
        s.season = rd<uint32_t>(schedule + offsetof(S, current_season), UINT32_MAX);
        s.season_over = rd<uint8_t>(schedule + offsetof(S, season_over)) != 0;
        uintptr_t b = rd<uintptr_t>(schedule + offsetof(S, seasons_begin));
        uintptr_t e = rd<uintptr_t>(schedule + offsetof(S, seasons_end));
        if (!b || e <= b || (e - b) % sizeof(Z) || (e - b) / sizeof(Z) > MAX_SEASONS || s.season >= (e - b) / sizeof(Z))
            return s.stop = "the season records do not read", s;
        uintptr_t season = b + s.season * sizeof(Z);
        uintptr_t cb = rd<uintptr_t>(season + offsetof(Z, driver_calendars_begin));
        uintptr_t ce = rd<uintptr_t>(season + offsetof(Z, driver_calendars_end));
        if (!cb || ce <= cb || (ce - cb) % sizeof(C) || (ce - cb) / sizeof(C) > MAX_CALENDARS)
            return s.stop = "the season's driver calendars do not read", s;
        for (uintptr_t c = cb; c < ce && !s.calendar; c += sizeof(C)) {
            uint32_t key = 0;
            if (mem::read(c + offsetof(C, driver), key) && same_key(key, s.driver))
                s.calendar = c;
        }
        if (!s.calendar)
            return s.stop = "no driver calendar for the human driver", s;
        hc::Layout L{offsetof(C, activities_begin),
                     offsetof(C, activities_end),
                     offsetof(A, day),
                     offsetof(A, state),
                     offsetof(A, activity),
                     offsetof(D, id),
                     offsetof(D, type),
                     offsetof(D, duration)};
        s.state_offset = L.item_state;
        s.day_offset = L.item_day;
        if (!hc::collect(s.calendar, s.day, L, read_raw, nullptr, s.today, &s.total))
            s.stop = "the calendar's activity list does not read";
        return s;
    } else {
        static_assert(missing_layout<S, Z, C, A, D>,
                      "the game headers lack a calendar field: ScheduleComponent.current_season or season_over, "
                      "CalendarActivity.state, ActivityDef.id or duration, or another field snapshot() reads");
        return s;
    }
}

void log_items(const Snapshot &s)
{
    for (size_t i = 0; i < s.today.size(); ++i)
        swofty::log("[career]   item %zu/%zu: %s", i + 1, s.today.size(), hc::describe(s.today[i]).c_str());
}

std::string summary(const Snapshot &s)
{
    char b[192];
    snprintf(b, sizeof b, "day %u, season %u%s, %zu of %zu calendar items running today (driver 0x%08x, calendar 0x%llx)",
             unsigned(s.day), s.season, s.season_over ? " (over)" : "", s.today.size(), s.total, s.driver,
             static_cast<unsigned long long>(s.calendar));
    return b;
}

std::string decision(const Snapshot &s) { return hc::describe(hc::decide(s.today, s.season_over), s.today, s.season_over); }

std::string calendar(const Snapshot &s)
{
    if (!s.stop.empty() && s.today.empty())
        return "the calendar is not readable (" + s.stop + ")";
    return summary(s) + "; the calendar check reads: " + decision(s);
}

bool env_zero(const char *name)
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return n > 0 && n < sizeof v && !std::strcmp(v, "0");
}

bool break_fix_off()
{
    static const bool off = env_zero("SWOFTY_HUB_BREAK_FIX");
    return off;
}

bool overlap_fix_off()
{
    static const bool off = env_zero("SWOFTY_HUB_OVERLAP_FIX");
    return off;
}

bool writable(uintptr_t p, size_t n)
{
    MEMORY_BASIC_INFORMATION mbi{};
    if (!p || VirtualQuery(reinterpret_cast<const void *>(p), &mbi, sizeof mbi) != sizeof mbi)
        return false;
    if (mbi.State != MEM_COMMIT || (mbi.Protect & PAGE_GUARD))
        return false;
    DWORD w = PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    return (mbi.Protect & w) && p + n <= reinterpret_cast<uintptr_t>(mbi.BaseAddress) + mbi.RegionSize;
}

bool ignore_break(const Snapshot &s, const hc::Item &it, const char *&how)
{
    uintptr_t at = uintptr_t(it.at + s.state_offset);
    uint8_t state = 0;
    if (break_fix_off())
        how = "left as it is (SWOFTY_HUB_BREAK_FIX=0)";
    else if (!s.state_offset || !mem::read(at, state) || state != hc::STARTED)
        how = "left as it is (its state no longer reads 1)";
    else if (!writable(at, sizeof state))
        how = "left as it is (not writable)";
    else {
        state = hc::IGNORED;
        std::memcpy(reinterpret_cast<void *>(at), &state, sizeof state);
        how = "set to ignored (state 4), so the race weekend decides";
        return true;
    }
    return false;
}

size_t fix_breaks(const Snapshot &s)
{
    hc::BreakFix f = hc::breaks_before_weekend(s.today, s.season_over);
    if (f.weekend < 0)
        return 0;
    const hc::Item &weekend = s.today[size_t(f.weekend)];
    size_t ignored = 0;
    for (size_t i : f.breaks) {
        const char *how = "";
        ignored += ignore_break(s, s.today[i], how);
        swofty::log("[career] season break over race weekend: %s runs over the race weekend days %u-%u (id %d) "
                    "that the session is in; %s",
                    hc::describe(s.today[i]).c_str(), unsigned(weekend.first), unsigned(weekend.last),
                    weekend.id == hc::NO_ID ? -1 : int(weekend.id), how);
    }
    return ignored;
}

std::string advance(const Snapshot &s)
{
    if (!s.stop.empty() && s.today.empty())
        return "Team HQ's Advance is not judged";
    return hc::describe_advance(s.today);
}

bool move_break(const Snapshot &s, const hc::Item &it, uint16_t first, const char *&how)
{
    uintptr_t state_at = uintptr_t(it.at + s.state_offset), day_at = uintptr_t(it.at + s.day_offset);
    uint8_t state = 0;
    uint16_t day = 0;
    if (break_fix_off())
        how = "left as it is (SWOFTY_HUB_BREAK_FIX=0)";
    else if (!s.state_offset || !s.day_offset || !mem::read(state_at, state) || state != hc::IGNORED || !mem::read(day_at, day) ||
             day != it.first)
        how = "left as it is (its state or day no longer reads as read)";
    else if (!writable(day_at, sizeof first))
        how = "left as it is (not writable)";
    else {
        std::memcpy(reinterpret_cast<void *>(day_at), &first, sizeof first);
        how = "moved back so it ends the day before";
        return true;
    }
    return false;
}

void team_hq_calendar()
{
    Snapshot s = snapshot();
    swofty::log("[career] Team HQ calendar: %s; %s", calendar(s).c_str(), advance(s).c_str());
    log_items(s);
    size_t moved = 0;
    for (const hc::BreakMove &m : hc::ended_breaks(s.today, s.day)) {
        const hc::Item &it = s.today[m.item];
        const char *how = "";
        moved += move_break(s, it, m.first, how);
        swofty::log("[career] ignored season break over Team HQ: %s keeps Team HQ's Advance off on day %u with no race "
                    "weekend running; to days %u-%u: %s",
                    hc::describe(it).c_str(), unsigned(s.day), unsigned(m.first), unsigned(m.first + (it.last - it.first)), how);
    }
    if (!moved)
        return;
    Snapshot after = snapshot();
    swofty::log("[career] Team HQ calendar after the season break move: %s; %s", calendar(after).c_str(), advance(after).c_str());
    log_items(after);
}

void opened()
{
    Snapshot s = snapshot();
    swofty::log("[career] hub opened: %s", calendar(s).c_str());
    log_items(s);
    if (!fix_breaks(s))
        return;
    Snapshot after = snapshot();
    swofty::log("[career] hub calendar after the season break fix: %s", calendar(after).c_str());
    log_items(after);
}

bool current_weekend(hc::Weekend &w)
{
    swofty_core::apply::moments::CareerWeekend m;
    if (!swofty_core::apply::moments::career_weekend(m))
        return false;
    w = {m.round, m.rounds, m.progress, m.current, m.sessions, m.complete, m.session, m.track, m.current_complete};
    return true;
}

void ended(uint64_t ms, uint64_t hub_ms, const char *how)
{
    hc::Weekend w;
    std::string weekend = current_weekend(w) ? hc::describe(w) : "the current weekend is not readable";
    Snapshot s = snapshot();
    swofty::log("[career] session ended %llu ms after start (%s; the career hub opened %llu ms after start): %s; %s",
                static_cast<unsigned long long>(ms), how, static_cast<unsigned long long>(hub_ms), weekend.c_str(),
                calendar(s).c_str());
    log_items(s);
}

bool complete_weekend(const Snapshot &s, const hc::Item &it, const char *&how)
{
    uintptr_t at = uintptr_t(it.at + s.state_offset);
    uint8_t state = 0;
    if (overlap_fix_off())
        how = "left as it is (SWOFTY_HUB_OVERLAP_FIX=0)";
    else if (!s.state_offset || !mem::read(at, state) || state != hc::STARTED)
        how = "left as it is (its state no longer reads 1)";
    else if (!writable(at, sizeof state))
        how = "left as it is (not writable)";
    else {
        state = hc::COMPLETED;
        std::memcpy(reinterpret_cast<void *>(at), &state, sizeof state);
        how = "set to completed (state 2), so the hub moves on";
        return true;
    }
    return false;
}

bool complete_overlap(const Snapshot &s, const hc::Weekend &w)
{
    hc::OverlapFix f = hc::finished_overlap(s.today, s.season_over, w);
    if (f.finished < 0)
        return false;
    const hc::Item &done = s.today[size_t(f.finished)], &next = s.today[size_t(f.next)];
    const char *how = "";
    bool set = complete_weekend(s, done, how);
    swofty::log(
        "[career] finished race weekend over the next one: %s is complete (round %u, %u of %u sessions) but still started "
        "while %s starts inside it; %s",
        hc::describe(done).c_str(), w.round, w.complete, w.sessions, hc::describe(next).c_str(), how);
    return set;
}

template <class C = g::DriverCalendar> bool calendar_driver(uintptr_t calendar, uint32_t &driver)
{
    if constexpr (requires(C *c) { c->driver; }) {
        return mem::read(calendar + offsetof(C, driver), driver);
    } else {
        (void)calendar, (void)driver;
        return false;
    }
}

template <class D = g::ActivityDef> uint8_t def_type(uintptr_t def)
{
    if constexpr (requires(D *d) { d->type; })
        return rd<uint8_t>(def + offsetof(D, type), 0xff);
    else
        return 0xff;
}

}

namespace runtime {

void hub_calendar_flow(uint32_t hash, uint32_t to)
{
    bool start = hash == hc::SESSION && to == SWOFTY_FLOW_ENTERING;
    bool hub = hash == hc::HUB && to == SWOFTY_FLOW_ENTERING;
    bool exited = hash == hc::SESSION && (to == SWOFTY_FLOW_EXITING || to == SWOFTY_FLOW_NONE);
    bool unload = hash == hc::UNLOAD && to == SWOFTY_FLOW_ENTERING;
    if (!start && !hub && !exited && !unload)
        return;
    uint64_t now = GetTickCount64(), ms = 0, hub_ms = 0;
    bool fast = false;
    {
        std::lock_guard<std::mutex> g(lock);
        if (start)
            watch.started(now);
        else if (hub)
            watch.hub(now);
        else
            fast = watch.ended(now, ms, hub_ms);
    }
    if (hub)
        opened();
    else if (fast)
        ended(ms, hub_ms, exited ? "StateSession exited" : "StateUnloadSession began first");
}

void hub_calendar_team_hq()
{
    hc::Weekend w;
    if (!current_weekend(w))
        swofty::log("[career] Team HQ weekend: the current weekend is not readable");
    else if (hc::never_ran(w))
        swofty::log("[career] weekend already started, session never ran: %s", hc::describe(w).c_str());
    else
        swofty::log("[career] Team HQ weekend: %s", hc::describe(w).c_str());
    team_hq_calendar();
}

void hub_calendar_load_loop()
{
    hc::Weekend w;
    bool known = current_weekend(w);
    std::string weekend = known ? hc::describe_loop(w) : "the current weekend is not readable";
    Snapshot s = snapshot();
    swofty::log("[career] load loop, first pass: %s; %s", weekend.c_str(), calendar(s).c_str());
    log_items(s);
    if (!known || !complete_overlap(s, w))
        return;
    Snapshot after = snapshot();
    swofty::log("[career] hub calendar after the finished weekend fix: %s", calendar(after).c_str());
    log_items(after);
}

void hub_calendar_added(void *calendar, const void *activity, uint32_t day)
{
    uint8_t type = def_type(reinterpret_cast<uintptr_t>(activity));
    if (!hc::logs_add(type))
        return;
    uint32_t key = 0, player = 0;
    bool known = calendar_driver(reinterpret_cast<uintptr_t>(calendar), key);
    bool mine = known && player_driver(player) && same_key(key, player);
    if (type == hc::CONTRACT_EVENT && !mine)
        return;
    {
        std::lock_guard<std::mutex> g(lock);
        if (!adds.fresh(reinterpret_cast<uintptr_t>(calendar), type, day))
            return;
    }
    uint16_t now = 0;
    bool clock = today(now);
    char when[32] = "unknown";
    if (clock)
        snprintf(when, sizeof when, "%u", unsigned(now));
    swofty::log("[career] calendar add: %s (0x%02x) on day %u, today %s, %s calendar 0x%llx (driver 0x%08x)", hc::type_name(type),
                type, day, when, mine ? "the human's" : "another",
                static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(calendar)), key);
}

}
