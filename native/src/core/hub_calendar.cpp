#include "runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#include "hub_calendar_logic.hpp"

// Career hub calendar diagnostic (hub_calendar_logic.hpp). When SessionStateCareerHub enters, the core logs the
// human's calendar activities running today and what the game's calendar check makes of them; when the hub leaves
// within AT_ONCE_MS it logs them again under "[career] hub left at once". career.calendar.add_activity reports each
// contract event or secret meeting added, with its day and today. Reads only, every one guarded: the ScheduleComponent
// (current season, season over, seasons), the season's driver calendars, their activities and definitions through
// KB structs (L2), and the clock through CareerTimeComponent.current_minutes. A build whose game headers lack a field
// logs that the calendar is not readable.

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

constexpr size_t MAX_SEASONS = 64;
constexpr size_t MAX_CALENDARS = 256;

std::mutex lock;
hc::Watch watch;
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

bool same_key(uint32_t a, uint32_t b)
{
    return ((a ^ b) & 0xffff00ffu) == 0;
}

struct Snapshot {
    std::string stop;
    uint32_t driver = 0, season = 0;
    uint16_t day = 0;
    bool season_over = false;
    uintptr_t calendar = 0;
    size_t total = 0;
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
        (void)day;
        return false;
    }
}

template <class S = g::ScheduleComponent, class Z = g::ScheduleSeason, class C = g::DriverCalendar,
          class A = g::CalendarActivity, class D = g::ActivityDef>
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
        hc::Layout L{offsetof(C, activities_begin), offsetof(C, activities_end), offsetof(A, day), offsetof(A, state),
                     offsetof(A, activity), offsetof(D, id), offsetof(D, type), offsetof(D, duration)};
        if (!hc::collect(s.calendar, s.day, L, read_raw, nullptr, s.today, &s.total))
            s.stop = "the calendar's activity list does not read";
        return s;
    } else {
        s.stop = "this core was built without the calendar layout";
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

std::string decision(const Snapshot &s)
{
    return hc::describe(hc::decide(s.today, s.season_over), s.today, s.season_over);
}

void opened()
{
    Snapshot s = snapshot();
    if (!s.stop.empty() && s.today.empty()) {
        swofty::log("[career] hub opened: the calendar is not readable (%s)", s.stop.c_str());
        return;
    }
    swofty::log("[career] hub opened: %s; the calendar check reads: %s", summary(s).c_str(), decision(s).c_str());
    log_items(s);
}

void left(uint64_t ms)
{
    Snapshot s = snapshot();
    if (!s.stop.empty() && s.today.empty()) {
        swofty::log("[career] hub left at once: %llu ms after it opened; the calendar is not readable (%s)",
                    static_cast<unsigned long long>(ms), s.stop.c_str());
        return;
    }
    swofty::log("[career] hub left at once: %llu ms after it opened; %s; the calendar check reads: %s",
                static_cast<unsigned long long>(ms), summary(s).c_str(), decision(s).c_str());
    log_items(s);
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
    if (hash != hc::HUB || (to != SWOFTY_FLOW_ENTERING && to != SWOFTY_FLOW_EXITING && to != SWOFTY_FLOW_NONE))
        return;
    uint64_t now = GetTickCount64(), ms = 0;
    bool at_once = false;
    {
        std::lock_guard<std::mutex> g(lock);
        if (to == SWOFTY_FLOW_ENTERING)
            watch.entered(now);
        else
            at_once = watch.left(now, ms);
    }
    if (to == SWOFTY_FLOW_ENTERING)
        opened();
    else if (at_once)
        left(ms);
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
    swofty::log("[career] calendar add: %s (0x%02x) on day %u, today %s, %s calendar 0x%llx (driver 0x%08x)",
                hc::type_name(type), type, day, when, mine ? "the human's" : "another",
                static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(calendar)), key);
}

}
