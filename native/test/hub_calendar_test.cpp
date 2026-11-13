// Host-native unit test for the core's career hub calendar diagnostic (native/src/core/hub_calendar_logic.hpp).
// Built and run by native/test/run_hub_calendar_test.sh.

#include "hub_calendar_logic.hpp"

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
    std::vector<Def> defs{{37, hc::SECRET_MEETING, 1}, {12, hc::RACE_WEEKEND, 4}, {9, 0x09, 1}, {5, 0x05, 1},
                          {0xffff, 0x0f, 1}};
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

static void watch_and_adds()
{
    hc::Watch w;
    uint64_t ms = 0;
    expect(!w.left(100, ms), "leaving without an entry reports nothing");
    w.entered(1000);
    expect(w.left(1400, ms) && ms == 400, "leaving 400 ms after the entry is at once");
    expect(!w.left(1500, ms), "one exit per entry");
    w.entered(2000);
    expect(!w.left(3000, ms) && ms == 1000, "leaving after AT_ONCE_MS is not at once");

    expect(hc::logs_add(hc::SECRET_MEETING) && hc::logs_add(hc::CONTRACT_EVENT) && !hc::logs_add(hc::RACE_WEEKEND),
           "secret meetings and contract events are logged, other adds are not");
    hc::AddLog a;
    expect(a.fresh(CAL, hc::SECRET_MEETING, 245), "the first add logs");
    expect(!a.fresh(CAL, hc::SECRET_MEETING, 245), "the same add again does not");
    expect(a.fresh(CAL, hc::SECRET_MEETING, 246), "another day logs");
    expect(a.fresh(CAL + 0xc8, hc::SECRET_MEETING, 246), "another calendar logs");
}

int main()
{
    secret_meeting_on_p1();
    meeting_inserted_ahead();
    states_and_edges();
    unreadable();
    lines();
    watch_and_adds();
    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
