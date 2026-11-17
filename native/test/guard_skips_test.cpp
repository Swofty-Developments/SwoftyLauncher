// Host-native unit test for the guards' shared first-N log caps (native/src/core/guards/skips.hpp) and switch values
// (native/src/core/guards/env.hpp). Built and run by native/test/run_tests.sh guard_skips.

#include "core/guards/env.hpp"
#include "core/guards/skips.hpp"

#include <cstdio>
#include <cstring>

namespace guards = swofty_core::guards;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

constinit guards::LogFirst<8> skips{"skips"};
constinit guards::LogFirst<3> ones{"ones"};
constinit guards::LogFirst<16> moves{"moves"};
constinit guards::LogFirst<4> plain;

int main()
{
    expect(!std::strcmp(skips.tail(), " (further skips are not logged)"), "the skips tail reads as the old literal");
    expect(!std::strcmp(ones.tail(), " (further ones are not logged)"), "the ones tail reads as the old literal");
    expect(!std::strcmp(moves.tail(), " (further moves are not logged)"), "the moves tail reads as the old literal");
    expect(!std::strcmp(plain.tail(), ""), "a cap without a noun has no tail");

    int lines = 0, tails = 0, plain_lines = 0;
    for (int i = 1; i <= 500; i++) {
        auto line = skips.note();
        if (line.n != i)
            expect(false, "every note gets the next number");
        if (line) {
            lines++;
            if (line.tail == skips.tail())
                tails++;
            else if (std::strcmp(line.tail, ""))
                expect(false, "lines before the cap have an empty tail");
        }
        if (bool(line) != (i <= 8))
            expect(false, "exactly the first eight notes are logged");
        if (plain.note())
            plain_lines++;
    }
    expect(lines == 8, "five hundred skips produce exactly the capped number of lines");
    expect(tails == 1, "the capped line carries the tail once");
    expect(skips.count() == 500, "every skip is counted");
    expect(plain_lines == 4, "a cap without a noun logs its first lines");

    for (int i = 1; i <= 3; i++) {
        auto line = ones.note();
        expect(bool(line) && (i == 3 ? line.tail == ones.tail() : !std::strcmp(line.tail, "")),
               "the cap of three marks its third line");
    }
    expect(!ones.note(), "the fourth note past a cap of three is silent");

    expect(guards::say(0, 4) == guards::Say::none, "occurrence zero is silent");
    expect(guards::say(1, 4) == guards::Say::line, "the first occurrence is logged plainly");
    expect(guards::say(4, 4) == guards::Say::last, "the last logged occurrence says the rest are silent");
    expect(guards::say(5, 4) == guards::Say::none, "occurrences past the cap are silent");

    guards::Skips<4> s;
    int logged = 0, last = 0;
    for (int i = 0; i < 100; i++) {
        int n = s.note();
        logged += s.logged(n);
        last += s.last(n);
    }
    expect(logged == 4 && last == 1 && s.count == 100, "the plain counter caps and marks like the runtime one");

    expect(guards::switch_off("0"), "0 switches a guard off");
    expect(guards::switch_off("off"), "off switches a guard off");
    expect(!guards::switch_off("1"), "1 leaves a guard on");
    expect(!guards::switch_off("false"), "false is not one of the shared off values");
    expect(!guards::switch_off("offx"), "a longer value is not off");
    expect(!guards::switch_off("00"), "00 is not off");
    expect(!guards::switch_off(""), "an empty value leaves a guard on");
    expect(!guards::switch_off(nullptr), "an unset value leaves a guard on");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
