// Host-native unit test for the core's career hub calendar diagnostic (native/src/core/career/hub_calendar_logic.hpp).
// Built and run by native/test/run_tests.sh hub_calendar.

#include "core/career/hub_calendar_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace hc = runtime::hub_calendar;

static int failed = 0;

static void expect(bool ok, const std::string &what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    if (!ok)
        failed++;
}

struct Memory {
    std::map<uint64_t, uint8_t> bytes;

    template <class T> void put(uint64_t at, T v)
    {
        uint8_t b[sizeof(T)];
        std::memcpy(b, &v, sizeof(T));
        for (size_t i = 0; i < sizeof(T); ++i)
            bytes[at + i] = b[i];
    }
};

static bool read(uint64_t at, void *out, size_t n, void *user)
{
    auto *m = static_cast<Memory *>(user);
    auto *o = static_cast<uint8_t *>(out);
    for (size_t i = 0; i < n; ++i) {
        auto it = m->bytes.find(at + i);
        if (it == m->bytes.end())
            return false;
        o[i] = it->second;
    }
    return true;
}

constexpr hc::Layout L{0x50, 0x58, 0x0a, 0x0c, 0x10, 0x08, 0x10, 0x12};
constexpr uint64_t CAL = 0x10000, LIST = 0x20000, ITEMS = 0x30000, DEFS = 0x40000;

struct Def {
    uint16_t id;
    uint8_t type;
    uint16_t duration;
};

static void calendar(Memory &m, const std::vector<std::pair<uint16_t, uint8_t>> &items, const std::vector<Def> &defs,
                     const std::vector<int> &def_of)
{
    for (size_t d = 0; d < defs.size(); ++d) {
        uint64_t at = DEFS + d * 0x100;
        m.put<uint16_t>(at + 0x08, defs[d].id);
        m.put<uint8_t>(at + 0x10, defs[d].type);
        m.put<uint16_t>(at + 0x12, defs[d].duration);
    }
    for (size_t i = 0; i < items.size(); ++i) {
        uint64_t at = ITEMS + i * 0x20;
        m.put<uint64_t>(at, 0x1456d10d0ull);
        m.put<uint16_t>(at + 0x0a, items[i].first);
        m.put<uint8_t>(at + 0x0c, items[i].second);
        m.put<uint64_t>(at + 0x10, DEFS + uint64_t(def_of[i]) * 0x100);
        m.put<uint64_t>(LIST + i * 16, 0);
        m.put<uint64_t>(LIST + i * 16 + 8, at);
    }
    m.put<uint64_t>(CAL + 0x50, LIST);
    m.put<uint64_t>(CAL + 0x58, LIST + items.size() * 16);
}

static void secret_meeting_on_p1()
{
    Memory m;
    std::vector<Def> defs{{12, hc::RACE_WEEKEND, 4}, {37, hc::SECRET_MEETING, 1}, {3, 0x06, 30}, {40, 0x0b, 2}};
    calendar(m, {{243, 1}, {243, 1}, {244, 1}, {246, 1}, {250, 1}}, defs, {2, 0, 1, 0, 3});
    std::vector<hc::Item> today;
    size_t total = 0;
    expect(hc::collect(CAL, 244, L, read, &m, today, &total), "a readable calendar collects");
    expect(total == 5, "it counts every calendar item");
    expect(today.size() == 3, "it keeps the items running today and stops at the first that starts later");
    expect(today.size() == 3 && today[0].type == 0x06 && today[0].last == 272, "a 30-day FreeTime runs days 243-272");
    expect(today.size() == 3 && today[1].type == hc::RACE_WEEKEND && today[1].first == 243 && today[1].last == 246,
           "the race weekend runs days 243-246");
    expect(today.size() == 3 && today[2].type == hc::SECRET_MEETING && today[2].first == 244 && today[2].last == 244,
           "the secret meeting is on day 244 only");
    hc::Decision d = hc::decide(today, false);
    expect(d.verdict == hc::Verdict::stay && d.by == 1,
           "FreeTime does not decide, so the race weekend ahead of the meeting keeps the hub");
    expect(hc::describe(d, today, false) == "stay in the hub (item 2, RaceWeekend, decides)",
           "the decision names its item: " + hc::describe(d, today, false));
}

static void meeting_inserted_ahead()
{
    Memory m;
    std::vector<Def> defs{{37, hc::SECRET_MEETING, 1}, {12, hc::RACE_WEEKEND, 4}};
    calendar(m, {{244, 1}, {244, 1}}, defs, {0, 1});
    std::vector<hc::Item> today;
    hc::collect(CAL, 244, L, read, &m, today);
    hc::Decision d = hc::decide(today, false);
    expect(d.verdict == hc::Verdict::team_hq && d.by == 0,
           "a secret meeting inserted ahead of the race weekend on its day sends the hub to Team HQ");
    expect(hc::describe(d, today, false) == "go to Team HQ (item 1, NSCSSM, decides)",
           "and the decision line says so: " + hc::describe(d, today, false));
}

static void states_and_edges()
{
    Memory m;
    std::vector<Def> defs{{37, hc::SECRET_MEETING, 1}, {12, hc::RACE_WEEKEND, 4}, {9, 0x09, 1}, {5, 0x05, 1}, {0xffff, 0x0f, 1}};
    calendar(m, {{244, 0}, {244, 1}}, defs, {0, 1});
    std::vector<hc::Item> today;
    hc::collect(CAL, 244, L, read, &m, today);
    expect(hc::decide(today, false).verdict == hc::Verdict::stay, "an item not in state 1 does not decide");
    expect(hc::decide(today, true).verdict == hc::Verdict::season_end && hc::decide(today, true).by == -1,
           "a season that is over answers the end of the season");
    expect(hc::decide({}, false).verdict == hc::Verdict::team_hq && hc::decide({}, false).by == -1,
           "nothing running today answers Team HQ");
    expect(hc::describe(hc::decide({}, false), {}, false) == "go to Team HQ (no running activity decides)",
           "and says no activity decided");

    Memory p;
    calendar(p, {{244, 1}, {244, 1}, {244, 1}}, defs, {4, 2, 3});
    hc::collect(CAL, 244, L, read, &p, today);
    hc::Decision d = hc::decide(today, false);
    expect(d.verdict == hc::Verdict::press && d.by == 1, "a definition without an id is skipped; a press interview answers 2");

    for (uint8_t t : {uint8_t(2), uint8_t(6), uint8_t(8), uint8_t(14)})
        expect(!hc::deciding(t), std::string(hc::type_name(t)) + " never decides");
    expect(hc::deciding(hc::SECRET_MEETING) && hc::deciding(hc::CONTRACT_EVENT) && hc::deciding(hc::RACE_WEEKEND),
           "secret meetings, contract events and race weekends decide");
    expect(!hc::deciding(hc::TYPE_COUNT), "an unknown type never decides");
}

static void unreadable()
{
    Memory m;
    std::vector<hc::Item> today;
    expect(!hc::collect(CAL, 1, L, read, &m, today), "a calendar that doesn't read reports false");
    m.put<uint64_t>(CAL + 0x50, LIST);
    m.put<uint64_t>(CAL + 0x58, LIST + 8);
    expect(!hc::collect(CAL, 1, L, read, &m, today), "a list whose size is not whole slots reports false");
    m.put<uint64_t>(CAL + 0x58, LIST + 16 * (hc::MAX_ITEMS + 1));
    expect(!hc::collect(CAL, 1, L, read, &m, today), "a list longer than MAX_ITEMS reports false");
    m.put<uint64_t>(CAL + 0x58, LIST + 32);
    m.put<uint64_t>(LIST + 8, 0);
    m.put<uint64_t>(LIST + 24, 0x99999);
    expect(hc::collect(CAL, 1, L, read, &m, today) && today.empty(), "null and unreadable items are skipped");
}

static void lines()
{
    hc::Item i;
    i.at = 0x1f2a3b4c0;
    i.vtable = 0x1456d10d0;
    i.def = 0x100;
    i.first = 244;
    i.last = 244;
    i.id = 37;
    i.type = hc::SECRET_MEETING;
    i.state = 1;
    expect(hc::describe(i) == "NSCSSM (0x10) days 244-244 state 1 id 37 at 0x1f2a3b4c0 vt 0x1456d10d0",
           "an item line: " + hc::describe(i));
    i.id = hc::NO_ID;
    i.type = 0x40;
    expect(hc::describe(i) == "unknown (0x40) days 244-244 state 1 id -1 at 0x1f2a3b4c0 vt 0x1456d10d0",
           "an unknown type and a missing id: " + hc::describe(i));
    expect(hc::HUB == 0x42843edbu, "the hub state's flow hash");
}

static void session_watch()
{
    expect(hc::SESSION == 0x1505f42au && hc::UNLOAD == 0xd432de2du, "the session and unload states' flow hashes");
    hc::SessionWatch w;
    uint64_t ms = 0, hub = 0;
    expect(!w.ended(100, ms, hub), "an end without a start reports nothing");

    w.started(15055002280ull);
    w.hub(15055002825ull);
    expect(w.ended(15055002870ull, ms, hub) && ms == 590 && hub == 545,
           "MorisGrimes' P1: the hub opened 545 ms in and the session ended 590 ms after start");
    expect(!w.ended(15055002900ull, ms, hub), "one end per start");

    w.started(1000);
    w.hub(1000);
    expect(!w.ended(400000, ms, hub) && ms == 399000, "a healthy session that ran for minutes is not reported");

    w.started(5000);
    w.hub(5000);
    expect(!w.ended(5000 + hc::FAST_END_MS, ms, hub), "an end at FAST_END_MS is not reported");

    w.started(9000);
    expect(!w.ended(9300, ms, hub), "a session without the career hub (Grand Prix, Time Trial) is not reported");

    w.hub(9400);
    w.started(10000);
    expect(!w.ended(10100, ms, hub), "a hub entry outside the session does not arm the next one");

    w.started(20000);
    w.hub(20100);
    w.hub(20900);
    expect(w.ended(21000, ms, hub) && hub == 100, "the first hub entry of the session counts");

    w.started(30000);
    w.hub(30200);
    w.started(30500);
    expect(!w.ended(30600, ms, hub), "a restart starts the session over without its hub");
}

static void weekends()
{
    hc::Weekend stuck{4, 17, hc::WEEKEND_IN_PROGRESS, 0, 7, 0, "practice_1", "game/silverstone"};
    expect(hc::never_ran(stuck), "a weekend in progress with no session complete never ran");
    expect(hc::describe(stuck) == "round 4/17, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions "
                                  "complete, track game/silverstone",
           "the stuck weekend's line: " + hc::describe(stuck));

    hc::Weekend next{5, 17, hc::WEEKEND_NOT_STARTED, 0, 7, 0, "practice_1", "game/hungaroring"};
    expect(!hc::never_ran(next), "a weekend not started yet is not stuck");

    hc::Weekend done{3, 17, hc::WEEKEND_COMPLETE, 6, 7, 7, "race", "game/austria"};
    expect(!hc::never_ran(done), "a complete weekend is not stuck");
    expect(hc::describe(done) == "round 3/17, weekend complete, current session 7 of 7 (race), 7 of 7 sessions complete, "
                                 "track game/austria",
           "a complete weekend's line: " + hc::describe(done));

    hc::Weekend mid{4, 17, hc::WEEKEND_IN_PROGRESS, 1, 7, 1, "practice_2", ""};
    expect(!hc::never_ran(mid), "a weekend with a session complete ran");
    expect(hc::describe(mid).find("track unknown") != std::string::npos, "a track not read says unknown");

    hc::Weekend odd{1, 1, 7, 0, 2, 0, "", ""};
    expect(hc::never_ran(odd) && hc::progress_name(7) == "progress state 7",
           "an unknown progress state with nothing complete counts as started and names its value");

    hc::Weekend unread{1, 1, hc::WEEKEND_IN_PROGRESS, 0, 0, 0, "", ""};
    expect(!hc::never_ran(unread), "a weekend whose sessions do not read is not called stuck");
    expect(hc::describe(unread) == "round 1/1, weekend in progress, its sessions do not read, track unknown",
           "an unread session list: " + hc::describe(unread));
}

static void adds()
{
    expect(hc::logs_add(hc::SECRET_MEETING) && hc::logs_add(hc::CONTRACT_EVENT) && !hc::logs_add(hc::RACE_WEEKEND),
           "secret meetings and contract events are logged, other adds are not");
    hc::AddLog a;
    expect(a.fresh(CAL, hc::SECRET_MEETING, 245), "the first add logs");
    expect(!a.fresh(CAL, hc::SECRET_MEETING, 245), "the same add again does not");
    expect(a.fresh(CAL, hc::SECRET_MEETING, 246), "another day logs");
    expect(a.fresh(CAL + 0xc8, hc::SECRET_MEETING, 246), "another calendar logs");
}

static void season_break_over_weekend()
{
    Memory m;
    std::vector<Def> defs{
        {40, hc::RACE_WEEKEND, 4}, {36, hc::SEASON_BREAK, 15}, {41, hc::RACE_WEEKEND, 4}, {42, hc::RACE_WEEKEND, 4}};
    calendar(m, {{162, 2}, {166, 1}, {169, 1}, {197, 0}}, defs, {0, 1, 2, 3});
    std::vector<hc::Item> today;
    expect(hc::collect(CAL, 170, L, read, &m, today) && today.size() == 2, "Fizzywoo's round 12, day 170: two items run today");
    expect(today.size() == 2 && today[0].type == hc::SEASON_BREAK && today[0].first == 166 && today[0].last == 180 &&
               today[0].id == 36,
           "item 1 is the 15-day SeasonBreak, days 166-180, id 36");
    expect(today.size() == 2 && today[1].type == hc::RACE_WEEKEND && today[1].first == 169 && today[1].last == 172 &&
               today[1].id == 41,
           "item 2 is the round 12 race weekend, days 169-172, id 41");
    hc::Decision d = hc::decide(today, false);
    expect(hc::describe(d, today, false) == "go to Team HQ (item 1, SeasonBreak, decides)",
           "the break decides Team HQ, as the 1.9.88 log read: " + hc::describe(d, today, false));

    hc::BreakFix f = hc::breaks_before_weekend(today, false);
    expect(f.breaks.size() == 1 && f.breaks[0] == 0 && f.weekend == 1, "the fix names the break ahead of the started weekend");
    expect(hc::breaks_before_weekend(today, true).weekend == -1, "nothing to do when the season is over");

    m.put<uint8_t>(today[0].at + L.item_state, hc::IGNORED);
    hc::collect(CAL, 170, L, read, &m, today);
    d = hc::decide(today, false);
    expect(d.verdict == hc::Verdict::stay && hc::describe(d, today, false) == "stay in the hub (item 2, RaceWeekend, decides)",
           "with the break ignored the weekend keeps the hub: " + hc::describe(d, today, false));
    expect(hc::breaks_before_weekend(today, false).weekend == -1 && hc::breaks_before_weekend(today, false).breaks.empty(),
           "the next session of the weekend finds nothing to do");

    for (uint16_t day : {uint16_t(166), uint16_t(168), uint16_t(173), uint16_t(180)}) {
        Memory q;
        calendar(q, {{162, 2}, {166, 1}, {169, 0}, {197, 0}}, defs, {0, 1, 2, 3});
        hc::collect(CAL, day, L, read, &q, today);
        expect(hc::breaks_before_weekend(today, false).weekend == -1,
               "a break with no started weekend running on day " + std::to_string(day) + " is left");
    }
}

static void season_break_left_alone()
{
    std::vector<Def> defs{{36, hc::SEASON_BREAK, 15}, {10, hc::RACE_WEEKEND, 4}, {37, hc::SECRET_MEETING, 1}, {40, 0x0b, 10},
                          {14, hc::RACE_WEEKEND, 4},  {6, hc::RACE_WEEKEND, 4}};
    std::vector<hc::Item> today;

    Memory primitive;
    calendar(primitive, {{343, 1}}, defs, {4});
    hc::collect(CAL, 344, L, read, &primitive, today);
    expect(hc::describe(hc::decide(today, false), today, false) == "stay in the hub (item 1, RaceWeekend, decides)" &&
               hc::breaks_before_weekend(today, false).weekend == -1,
           "Primitive's day 344, one race weekend: stays, nothing to do");

    Memory tutusenna;
    calendar(tutusenna, {{22, 1}}, defs, {5});
    hc::collect(CAL, 23, L, read, &tutusenna, today);
    expect(hc::decide(today, false).verdict == hc::Verdict::stay && hc::breaks_before_weekend(today, false).weekend == -1,
           "tutusenna's day 23, one race weekend: stays, nothing to do");

    Memory first;
    calendar(first, {{164, 1}, {166, 1}}, defs, {1, 0});
    hc::collect(CAL, 166, L, read, &first, today);
    expect(hc::decide(today, false).verdict == hc::Verdict::stay && hc::breaks_before_weekend(today, false).weekend == -1,
           "a weekend that starts before the break (2013 round 10, days 164-167) decides first and is left");

    Memory meeting;
    calendar(meeting, {{166, 1}, {169, 1}, {169, 1}}, defs, {0, 2, 1});
    hc::collect(CAL, 169, L, read, &meeting, today);
    expect(today.size() == 3 && hc::breaks_before_weekend(today, false).weekend == -1,
           "a break with a secret meeting ahead of the weekend is left: ignoring it would not keep the hub");

    Memory dynamic;
    calendar(dynamic, {{166, 1}, {169, 1}}, defs, {3, 1});
    hc::collect(CAL, 170, L, read, &dynamic, today);
    expect(hc::decide(today, false).verdict == hc::Verdict::team_hq && hc::breaks_before_weekend(today, false).weekend == -1,
           "a DynamicBreak ahead of the weekend is not a season break and is left");

    Memory twice;
    calendar(twice, {{166, 4}, {169, 2}, {176, 1}}, defs, {0, 1, 1});
    hc::collect(CAL, 177, L, read, &twice, today);
    expect(hc::decide(today, false).verdict == hc::Verdict::stay && hc::breaks_before_weekend(today, false).weekend == -1,
           "2020's second round in the break (days 176-179) finds the break already ignored");

    Memory two;
    calendar(two, {{160, 1}, {166, 1}, {169, 1}}, defs, {0, 0, 1});
    hc::collect(CAL, 170, L, read, &two, today);
    hc::BreakFix f = hc::breaks_before_weekend(today, false);
    expect(f.breaks.size() == 2 && f.weekend == 2, "every started break ahead of the weekend is named");

    hc::Item i;
    i.def = 0x100;
    i.id = 36;
    i.type = hc::SEASON_BREAK;
    i.state = hc::STARTED;
    expect(hc::counts(i), "a started season break with an id counts");
    i.state = hc::IGNORED;
    expect(!hc::counts(i), "an ignored one does not");
}

static void season_break_over_team_hq()
{
    std::vector<Def> defs{{40, hc::RACE_WEEKEND, 4},
                          {36, hc::SEASON_BREAK, 15},
                          {41, hc::RACE_WEEKEND, 4},
                          {42, hc::RACE_WEEKEND, 4},
                          {50, 0x0b, 10},
                          {9, 0x09, 1}};
    std::vector<hc::Item> today;

    Memory m;
    calendar(m, {{161, 2}, {166, 4}, {168, 2}, {189, 0}}, defs, {0, 1, 2, 3});
    expect(hc::collect(CAL, 172, L, read, &m, today) && today.size() == 1 && today[0].type == hc::SEASON_BREAK &&
               today[0].state == hc::IGNORED,
           "szariq_'s Team HQ after round 12 (days 168-171), day 172: only the ignored break 166-180 runs");
    expect(hc::advance_blocker(today) == 0 &&
               hc::describe_advance(today) == "Team HQ's Advance is off (item 1, SeasonBreak, keeps it off)",
           "the ignored break keeps Advance off: " + hc::describe_advance(today));
    expect(hc::decide(today, false).verdict == hc::Verdict::team_hq, "and the hub check, which needs state 1, ignores it");
    std::vector<hc::BreakMove> mv = hc::ended_breaks(today, 172);
    expect(mv.size() == 1 && mv[0].item == 0 && mv[0].first == 157, "the move puts the break on days 157-171");

    m.put<uint16_t>(today[0].at + L.item_day, mv[0].first);
    hc::collect(CAL, 172, L, read, &m, today);
    expect(today.empty() && hc::advance_blocker(today) == -1 &&
               hc::describe_advance(today) == "Team HQ's Advance is on (nothing running today keeps it off)",
           "after the move nothing runs on day 172 and Advance is on");
    hc::collect(CAL, 190, L, read, &m, today);
    expect(today.size() == 1 && today[0].type == hc::RACE_WEEKEND && today[0].first == 189,
           "round 13 still collects past the moved break, which now sits out of day order");
    expect(hc::ended_breaks(today, 190).empty(), "and its weekend is left alone");

    Memory twice;
    calendar(twice, {{166, 4}, {169, 2}, {176, 0}}, defs, {1, 2, 3});
    hc::collect(CAL, 173, L, read, &twice, today);
    mv = hc::ended_breaks(today, 173);
    expect(mv.size() == 1 && mv[0].first == 158, "2020's first round in the break done, day 173: the break moves to 158-172");
    twice.put<uint16_t>(today[0].at + L.item_day, mv[0].first);
    hc::collect(CAL, 177, L, read, &twice, today);
    expect(today.size() == 1 && today[0].first == 176 && hc::breaks_before_weekend(today, false).weekend == -1,
           "and its second round (days 176-179) runs with no break");

    Memory weekend;
    calendar(weekend, {{166, 4}, {168, 1}}, defs, {1, 2});
    hc::collect(CAL, 171, L, read, &weekend, today);
    expect(today.size() == 2 && hc::ended_breaks(today, 171).empty(),
           "on the weekend's own days the race weekend keeps Advance off and the break is left");

    Memory started;
    calendar(started, {{166, 1}}, defs, {1});
    hc::collect(CAL, 172, L, read, &started, today);
    expect(hc::advance_blocker(today) == 0 && hc::ended_breaks(today, 172).empty(),
           "a break in state 1 keeps Advance off and is left to the game's break flow");

    Memory dynamic;
    calendar(dynamic, {{166, 4}, {170, 1}}, defs, {1, 4});
    hc::collect(CAL, 172, L, read, &dynamic, today);
    expect(today.size() == 2 && hc::ended_breaks(today, 172).empty(),
           "a break ignored inside a running DynamicBreak, the game's own case, is left");

    Memory press;
    calendar(press, {{166, 4}, {172, 1}}, defs, {1, 5});
    hc::collect(CAL, 172, L, read, &press, today);
    std::vector<hc::BreakMove> p = hc::ended_breaks(today, 172);
    expect(hc::advance_blocker(today) == 0 && p.size() == 1 && p[0].item == 0,
           "a press interview does not keep Advance off, so the break beside it still moves");

    Memory early;
    calendar(early, {{0, 4}}, defs, {1});
    hc::collect(CAL, 10, L, read, &early, today);
    expect(today.size() == 1 && hc::ended_breaks(today, 10).empty(), "a break longer than the days before today is left");

    for (uint8_t t : {hc::END_SEASON, hc::RACE_WEEKEND, hc::DYNAMIC_BREAK, hc::SEASON_BREAK})
        expect(hc::blocks_advance(t), std::string(hc::type_name(t)) + " keeps Advance off");
    for (uint8_t t : {uint8_t(0x09), hc::CONTRACT_EVENT, hc::SECRET_MEETING, uint8_t(0x06)})
        expect(!hc::blocks_advance(t), std::string(hc::type_name(t)) + " does not");
}

int main()
{
    secret_meeting_on_p1();
    meeting_inserted_ahead();
    states_and_edges();
    unreadable();
    lines();
    session_watch();
    weekends();
    adds();
    season_break_over_weekend();
    season_break_left_alone();
    season_break_over_team_hq();
    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
