// Host-native unit test for the core's pit box and chassis registry guards (native/src/core/pit_box_logic.hpp).
// Built and run by native/test/run_pit_box_test.sh.

#include "pit_box_logic.hpp"

#include <cstdio>

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
    expect(pb::box_usable(39), "the last of the 40 slots is usable");
    expect(!pb::box_usable(40), "box 40 is past the slot array");
    expect(!pb::box_usable(99), "the no-box value 99 is not usable");
    expect(!pb::box_usable(uint32_t(-1)), "a third car's box of -1 is not usable");

    expect(!pb::registry_full(0), "an empty registry takes a body");
    expect(!pb::registry_full(25), "the 26th body is registered");
    expect(pb::registry_full(26), "the 27th body is skipped");
    expect(pb::registry_full(30), "a count past the slots stays skipped");

    pb::Seen seen;
    expect(seen.note(uint32_t(-1)) == pb::Log::line, "the first bad box is logged");
    expect(seen.note(uint32_t(-1)) == pb::Log::none, "the same box is logged once");
    expect(seen.note(99) == pb::Log::line, "another box is logged");
    pb::Seen many;
    int lines = 0;
    for (uint32_t b = 40; b < 240; b++)
        if (many.note(b) != pb::Log::none)
            lines++;
    expect(lines == pb::LOG_FIRST, "two hundred boxes produce exactly the capped number of lines");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
