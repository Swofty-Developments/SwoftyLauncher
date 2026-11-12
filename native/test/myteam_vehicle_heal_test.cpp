// Host-native unit test for the core's My Team vehicle heal (native/src/core/myteam_vehicle_heal_logic.hpp).
// Built and run by native/test/run_myteam_vehicle_heal_test.sh.

#include "myteam_vehicle_heal_logic.hpp"

#include <cstdio>
#include <map>
#include <string>
#include <vector>

namespace mh = runtime::myteam_heal;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Fake {
    std::map<uint64_t, uint64_t> q;
    std::map<uint64_t, uint32_t> d;
    std::map<uint64_t, std::string> s;

    bool u64(uint64_t a, uint64_t &out)
    {
        auto it = q.find(a);
        if (it == q.end())
            return false;
        out = it->second;
        return true;
    }

    bool u32(uint64_t a, uint32_t &out)
    {
        auto it = d.find(a);
        if (it == d.end())
            return false;
        out = it->second;
        return true;
    }

    bool text(uint64_t a, std::string &out)
    {
        auto it = s.find(a);
        if (it == s.end())
            return false;
        out = it->second;
        return true;
    }
};

constexpr uint64_t ENTRANTS = 0x340d01000, STOCK_TABLE = 0x340e00000, CUSTOM_TABLE = 0x340e10000, STOCK_SLOTS = 0x341000000,
                   CUSTOM_SLOTS = 0x342000000, VEHICLES = 0x350000000, LIST = 0x360000000;

static uint64_t car(int i)
{
    return VEHICLES + static_cast<uint64_t>(i) * 0x1000;
}

static uint32_t stock_handle(uint16_t id)
{
    return static_cast<uint32_t>(id) << 16;
}

static uint32_t custom_handle(uint16_t id)
{
    return static_cast<uint32_t>(id) << 16 | mh::KIND_CUSTOM;
}

static Fake game(const std::vector<std::string> &stock, size_t custom_slots)
{
    Fake f;
    f.q[ENTRANTS + mh::TABLES] = STOCK_TABLE;
    f.q[ENTRANTS + mh::TABLES + mh::TABLE_STRIDE] = CUSTOM_TABLE;
    f.q[ENTRANTS + mh::TABLES + 2 * mh::TABLE_STRIDE] = 0;
    f.q[STOCK_TABLE + mh::SLOTS_BEGIN] = STOCK_SLOTS;
    f.q[STOCK_TABLE + mh::SLOTS_END] = STOCK_SLOTS + stock.size() * mh::SLOT_SIZE;
    for (size_t i = 0; i < stock.size(); i++) {
        f.q[STOCK_SLOTS + i * mh::SLOT_SIZE + mh::SLOT_VEHICLE] = stock[i].empty() ? 0 : car(static_cast<int>(i));
        if (!stock[i].empty())
            f.s[car(static_cast<int>(i)) + mh::VEHICLE_FILE_NAME] = stock[i];
    }
    f.q[CUSTOM_TABLE + mh::SLOTS_BEGIN] = CUSTOM_SLOTS;
    f.q[CUSTOM_TABLE + mh::SLOTS_END] = CUSTOM_SLOTS + custom_slots * mh::SLOT_SIZE;
    for (size_t i = 0; i < custom_slots; i++)
        f.q[CUSTOM_SLOTS + i * mh::SLOT_SIZE + mh::SLOT_VEHICLE] = 0;
    return f;
}

static void entrants(Fake &f, const std::vector<uint32_t> &handles)
{
    for (size_t i = 0; i < handles.size(); i++)
        f.d[LIST + i * mh::ENTRANT_SIZE + mh::ENTRANT_VEHICLE] = handles[i];
}

static uint64_t list_end(size_t n)
{
    return LIST + n * mh::ENTRANT_SIZE;
}

int main()
{
    expect(mh::kind(0x4000) == 0 && mh::kind(custom_handle(7)) == 1 && mh::id(custom_handle(7)) == 7,
           "a vehicle handle is the kind in its low byte and the stat id in its high word");
    expect(mh::healable(custom_handle(0)) && mh::healable(custom_handle(8)), "My Team handles (kind 1) are healed");
    expect(!mh::healable(stock_handle(5)) && !mh::healable(0x3fff0000u), "stock handles and the empty handle are not");
    expect(!mh::healable(custom_handle(0x3fff)) && !mh::healable(custom_handle(mh::MAX_ID)), "an out-of-range custom id is not");

    std::vector<std::string> stock = {"ferrari",           "mercedes",          "",
                                      "myteam_ferrari",    "myteam_mercedes",   "myteam_honda",
                                      "ferrari_2026",      "mercedes_2026",     "myteam_mes_nissan_2026",
                                      "myteam_honda_2026", "myteam_mercedes_2026"};
    Fake f = game(stock, 2);
    expect(mh::vehicle(f, ENTRANTS, stock_handle(1)) == car(1), "a stock handle resolves through the kind 0 table");
    expect(mh::vehicle(f, ENTRANTS, stock_handle(2)) == 0, "an empty stock slot resolves to nothing");
    expect(mh::vehicle(f, ENTRANTS, stock_handle(99)) == 0, "a stat id past the table resolves to nothing");
    expect(mh::vehicle(f, ENTRANTS, custom_handle(1)) == 0, "silversquared's lost My Team car (custom 1) resolves to nothing");
    expect(mh::vehicle(f, ENTRANTS, 0x00000002u) == 0, "a kind with no table resolves to nothing");

    mh::Stock s = mh::stock(f, ENTRANTS);
    expect(s.myteam.size() == 5, "the stock My Team cars are listed, the empty slot, other cars and addon cars are not");
    bool mes = false;
    for (const auto &e : s.myteam)
        mes = mes || e.second == "myteam_mes_nissan_2026";
    expect(!mes, "a More Engine Suppliers car is never a source");

    entrants(f, {stock_handle(6), stock_handle(7), custom_handle(1), custom_handle(1)});
    expect(mh::season_2026(f, ENTRANTS, LIST, list_end(4)), "a session of 2026 stock cars is a 2026 season");
    mh::Source src;
    expect(mh::plan(f, ENTRANTS, LIST, list_end(4), src) && src.label == "myteam_mercedes_2026" && src.vehicle == car(10) && src.y2026,
           "a 2026 session rebuilds the car from myteam_mercedes_2026");

    Fake f25 = game(stock, 2);
    entrants(f25, {stock_handle(0), stock_handle(1), custom_handle(0), custom_handle(0)});
    expect(!mh::season_2026(f25, ENTRANTS, LIST, list_end(4)), "a session of 2025 stock cars is not a 2026 season");
    expect(mh::plan(f25, ENTRANTS, LIST, list_end(4), src) && src.label == "myteam_mercedes" && src.vehicle == car(4) && !src.y2026,
           "a 2025 session rebuilds the car from myteam_mercedes");

    std::vector<std::string> no_mercedes26 = {"ferrari_2026", "myteam_mercedes", "myteam_audi_2026", "myteam_honda_2026"};
    Fake fa = game(no_mercedes26, 1);
    entrants(fa, {stock_handle(0), custom_handle(0)});
    expect(mh::plan(fa, ENTRANTS, LIST, list_end(2), src) && src.label == "myteam_honda_2026",
           "without myteam_mercedes_2026 the next 2026 My Team car in the preference list is used");

    std::vector<std::string> other26 = {"ferrari_2026", "myteam_mercedes", "myteam_cadillac_2026"};
    Fake fo = game(other26, 1);
    entrants(fo, {stock_handle(0), custom_handle(0)});
    expect(mh::plan(fo, ENTRANTS, LIST, list_end(2), src) && src.label == "myteam_cadillac_2026",
           "a 2026 session prefers any 2026 My Team car over a 2025 one");

    std::vector<std::string> only25 = {"ferrari_2026", "myteam_ferrari"};
    Fake fe = game(only25, 1);
    entrants(fe, {stock_handle(0), custom_handle(0)});
    expect(mh::plan(fe, ENTRANTS, LIST, list_end(2), src) && src.label == "myteam_ferrari" && src.y2026,
           "a 2026 session with only 2025 My Team cars takes a 2025 one");

    std::vector<std::string> none = {"ferrari", "mercedes", "myteam_mes_porsche_2026"};
    Fake fn = game(none, 1);
    entrants(fn, {stock_handle(0), custom_handle(0)});
    expect(!mh::plan(fn, ENTRANTS, LIST, list_end(2), src), "with no stock My Team car there is nothing to rebuild from");

    Fake empty_list = game(stock, 2);
    expect(!mh::season_2026(empty_list, ENTRANTS, 0, 0) && !mh::season_2026(empty_list, ENTRANTS, LIST, LIST),
           "an empty or unreadable entrant list reads as a 2025 season");

    Fake no_tables;
    expect(mh::vehicle(no_tables, ENTRANTS, custom_handle(0)) == 0 && mh::stock(no_tables, ENTRANTS).myteam.empty(),
           "an unreadable entrants system resolves nothing and lists no source");

    Fake torn = game(stock, 2);
    torn.q[STOCK_TABLE + mh::SLOTS_END] = STOCK_SLOTS + 8;
    expect(mh::stock(torn, ENTRANTS).myteam.empty(), "a slot vector whose size is not whole slots is not read");

    expect(mh::replaced(0) && mh::replaced(2) && mh::replaced(4), "ReplaceVehicle's replaced, added and unchanged results count");
    expect(!mh::replaced(1) && !mh::replaced(3) && !mh::replaced(5), "its no-table, failed-store and preloaded results do not");

    mh::Ids ids;
    expect(ids.first(1) && !ids.first(1) && ids.first(8), "each rebuilt car is logged once");
    for (uint16_t i = 100; ids.seen.size() < mh::LOG_IDS; i++)
        ids.first(i);
    expect(!ids.first(999), "cars past 32 are not logged");

    expect(mh::switch_off("0") && mh::switch_off("off") && !mh::switch_off("1") && !mh::switch_off(nullptr), "the switch reads 0 or off");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
