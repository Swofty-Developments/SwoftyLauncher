// Host-native unit test for the core's pit box and chassis registry guards (native/src/core/guards/garage/pit_box_logic.hpp).
// Built and run by native/test/run_tests.sh pit_box.

#include "core/guards/garage/pit_box_logic.hpp"

#include <cstdio>
#include <cstring>

namespace pb = runtime::pit_box;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    expect(pb::box_usable(0), "box 0 has a spline slot");
    expect(pb::box_usable(23), "the last of the 24 slots is usable");
    expect(!pb::box_usable(24), "box 24 is past the slot array, where the tuning parameter handles start");
    expect(!pb::box_usable(39), "box 39 is past the slot array");
    expect(!pb::box_usable(40), "box 40 is past the slot array");
    expect(!pb::box_usable(99), "the no-box value 99 is not usable");
    expect(!pb::box_usable(uint32_t(-1)), "a third car's box of -1 is not usable");

    expect(!pb::registry_full(0), "an empty registry takes a body");
    expect(!pb::registry_full(25), "the 26th body is registered");
    expect(pb::registry_full(26), "the 27th body is skipped");
    expect(pb::registry_full(30), "a count past the slots stays skipped");

    {
        uint64_t sc = 0x1111, box13 = 0x2222;
        pb::Redirect r;
        expect(r.engage(&sc, &box13), "a PitBox13 placement redirects box 12");
        expect(sc == 0x2222, "the safety car's slot holds PitBox13's placement during the call");
        r.release();
        expect(sc == 0x1111 && box13 == 0x2222, "the safety car's placement is back after the call");
        r.release();
        expect(sc == 0x1111, "a second release changes nothing");
    }
    {
        uint64_t sc = 0x1111, none = 0;
        pb::Redirect r;
        expect(!r.engage(&sc, &none), "no PitBox13 leaves box 12 on the safety car's spot");
        expect(sc == 0x1111, "the slot is untouched without PitBox13");
        uint64_t same = 0x1111;
        expect(!r.engage(&sc, &same), "the same placement in both slots needs no redirect");
        expect(!r.engage(nullptr, &same) && !r.engage(&sc, nullptr), "a missing slot pointer needs no redirect");
        r.release();
        expect(sc == 0x1111, "releasing an unengaged redirect changes nothing");
    }
    expect(pb::SAFETY_CAR_SLOT == 0x738 && pb::BOX_13_SLOT == 0x898, "the slots are type 7 slot 0 and type 8 slot 12");

    pb::Seen seen{"repairs"};
    auto first = seen.note(uint32_t(-1));
    expect(first && !std::strcmp(first.tail, ""), "the first bad box is logged");
    expect(!seen.note(uint32_t(-1)), "the same box is logged once");
    expect(bool(seen.note(99)), "another box is logged");
    pb::Seen many{"repairs"};
    size_t lines = 0;
    const char *last = nullptr;
    for (uint32_t b = 40; b < 240; b++)
        if (auto line = many.note(b)) {
            lines++;
            last = line.tail;
        }
    expect(lines == pb::LOG_FIRST, "two hundred boxes produce exactly the capped number of lines");
    expect(last && !std::strcmp(last, " (further repairs are not logged)"), "the last line says further repairs are not logged");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
