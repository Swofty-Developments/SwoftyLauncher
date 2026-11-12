// Host-native unit test for the core's career calendar track list guard (native/src/core/track_list_guard_logic.hpp):
// the game's custom_track_list load loop is replayed over event rows that share ids, with and without the guard, across
// save and load cycles, and for a save that already holds a blown-up list.
// Built and run by native/test/run_track_list_guard_test.sh.

#include "track_list_guard_logic.hpp"

#include <cstdio>
#include <set>
#include <string>
#include <vector>

namespace tl = runtime::track_list_guard;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Row {
    uint8_t id;
};

static std::vector<size_t> load(const std::vector<Row> &rows, const std::vector<uint8_t> &stored, tl::Load *guard, uintptr_t component,
                                size_t *repeats = nullptr)
{
    std::vector<size_t> calendar;
    for (size_t e = 0; e < stored.size(); e++) {
        for (size_t r = 0; r < rows.size(); r++) {
            if (rows[r].id != stored[e])
                continue;
            if (guard && !tl::take(*guard, component, calendar.size(), 0x10000 + r * tl::ENTRY))
                continue;
            calendar.push_back(r);
        }
        if (guard && tl::last_entry(e, stored.size()) && repeats)
            *repeats = tl::finish(*guard, component);
    }
    return calendar;
}

static std::vector<uint8_t> save(const std::vector<Row> &rows, const std::vector<size_t> &calendar)
{
    std::vector<uint8_t> out;
    for (size_t r : calendar)
        out.push_back(rows[r].id);
    return out;
}

int main()
{
    std::vector<Row> stock;
    for (uint8_t id = 0; id < 24; id++)
        stock.push_back({id});
    std::vector<uint8_t> calendar_ids = {0, 3, 7, 12, 23};
    tl::Load guard;

    auto plain = load(stock, calendar_ids, nullptr, 1);
    auto guarded = load(stock, calendar_ids, &guard, 1);
    expect(plain == guarded, "stock events (one per id): the guarded load equals the game's load");
    expect(save(stock, guarded) == calendar_ids, "stock events: the calendar saves back to the stored ids");

    std::vector<Row> seasons;
    for (int season = 0; season < 10; season++)
        for (uint8_t id = 0; id < 24; id++)
            seasons.push_back({id});

    std::vector<uint8_t> ids = calendar_ids;
    size_t stock_size = 0;
    for (int cycle = 0; cycle < 4; cycle++) {
        auto cal = load(seasons, ids, nullptr, 1);
        stock_size = cal.size();
        ids = save(seasons, cal);
    }
    expect(stock_size == 5 * 10 * 10 * 10 * 10, "10 seasons sharing ids: the game's own load multiplies the calendar 10x per cycle");

    ids = calendar_ids;
    std::vector<size_t> first;
    bool flat = true;
    for (int cycle = 0; cycle < 20; cycle++) {
        size_t repeats = 0;
        auto cal = load(seasons, ids, &guard, 1, &repeats);
        if (cycle == 0)
            first = cal;
        else if (cal != first)
            flat = false;
        ids = save(seasons, cal);
    }
    expect(first.size() == 50, "10 seasons sharing ids: the guarded load keeps every event the game's first load has (5 ids x 10)");
    expect(flat, "10 seasons sharing ids: the guarded calendar stays the same over 20 save and load cycles");
    expect(std::set<size_t>(first.begin(), first.end()).size() == first.size(), "the guarded calendar holds each event row once");

    auto game_first = load(seasons, calendar_ids, nullptr, 1);
    expect(game_first == first, "the guarded calendar is the game's first load, in the game's order");

    std::vector<uint8_t> blown;
    for (size_t i = 0; i < 65535; i++)
        blown.push_back(calendar_ids[i % calendar_ids.size()]);
    size_t repeats = 0;
    auto healed = load(seasons, blown, &guard, 2, &repeats);
    expect(healed.size() == 50, "a blown-up save (65,535 stored entries) loads back to one entry per event (50)");
    expect(repeats == 65535 * 10 - 50, "the load reports the repeat matches it skipped");
    expect(load(seasons, save(seasons, healed), &guard, 2) == healed, "the healed calendar saves and loads unchanged");
    std::set<size_t> stock_rows;
    for (size_t r : load(seasons, std::vector<uint8_t>(blown.begin(), blown.begin() + 5), nullptr, 2))
        stock_rows.insert(r);
    expect(std::set<size_t>(healed.begin(), healed.end()) == stock_rows, "the healed calendar keeps every event the save names");

    tl::Load reset;
    expect(tl::take(reset, 1, 0, 0x100), "the first push of a load is taken");
    expect(!tl::take(reset, 1, 1, 0x100), "the same row again in that load is skipped");
    expect(tl::take(reset, 1, 0, 0x100), "an empty calendar (a new load) starts over");
    expect(tl::take(reset, 2, 1, 0x100), "another component starts over");
    expect(tl::finish(reset, 3) == 0, "finishing another component reports no repeats");
    expect(reset.rows.empty() && reset.component == 0, "finish clears the load state");

    expect(tl::last_entry(4, 5) && !tl::last_entry(3, 5), "the last stored entry is index count - 1");
    expect(tl::entries(0x1000, 0x1000 + 3 * tl::ENTRY) == 3 && tl::entries(0x2000, 0x1000) == 0, "calendar size from begin and end");

    tl::SaveLog log;
    expect(log.changed(0xa, 24), "the first save of a component is logged");
    expect(!log.changed(0xa, 24), "a save with the same count is not logged again");
    expect(log.changed(0xa, 25), "a save with a new count is logged");
    expect(log.changed(0xb, 24), "another component's first save is logged");

    std::string line = tl::load_line(0x1234, 65535, 50, 655300, true);
    expect(line.find("65535 stored entries -> 50 calendar events") != std::string::npos && line.find("655300 repeat") != std::string::npos &&
               line.find("would load 655350") != std::string::npos,
           "the load line names the stored count, the calendar size and the skipped repeats");
    expect(tl::load_line(0x1234, 5, 5, 0, true).find("no repeats") != std::string::npos, "a clean load says no repeats");
    expect(tl::save_line(0x1234, 24).find("24 calendar events") != std::string::npos, "the save line names the count");

    expect(tl::switch_off("0") && tl::switch_off("off"), "SWOFTY_TRACK_LIST_GUARD=0 or off turns it off");
    expect(!tl::switch_off("1") && !tl::switch_off("") && !tl::switch_off(nullptr), "anything else leaves it on");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
