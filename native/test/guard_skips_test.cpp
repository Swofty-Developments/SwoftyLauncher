// Host-native unit test for the guards' shared first-N log caps and first-seen sets (native/src/core/guards/skips.hpp)
// and switch values (native/src/core/guards/env.hpp). Built and run by native/test/run_tests.sh guard_skips.

#include "core/guards/env.hpp"
#include "core/guards/skips.hpp"

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <utility>
#include <vector>

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
constinit guards::FirstKeys<uint32_t, 4> rounds{"rounds"};
constinit guards::FirstKeys<std::pair<uintptr_t, int>, 16> weekends;
guards::FirstKeys<std::string, 32> threaded{"vehicles"};

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

    expect(!std::strcmp(rounds.tail(), " (further rounds are not logged)"), "a key cap builds its tail like a count cap");
    auto first = rounds.note(7);
    expect(first && first.n == 1 && !std::strcmp(first.tail, ""), "a key's first note is logged plainly");
    expect(!rounds.note(7), "a key noted before is silent");
    expect(rounds.note(8) && rounds.note(9), "new keys are logged");
    auto fourth = rounds.note(10);
    expect(fourth && fourth.n == 4 && fourth.tail == rounds.tail(), "the key that fills the cap carries the tail");
    expect(!rounds.note(11) && !rounds.note(7), "new keys past the cap and old keys are silent");
    expect(rounds.size() == 4, "keys past the cap are not kept");

    int weekend_lines = 0;
    for (int pass = 0; pass < 3; pass++)
        for (uintptr_t w = 1; w <= 40; w++)
            weekend_lines += bool(weekends.note({w * 0x1000, int(w % 3)}));
    expect(weekend_lines == 16 && weekends.size() == 16, "pair keys log the first sixteen distinct weekends once");
    expect(!std::strcmp(weekends.tail(), ""), "a key cap without a noun has no tail");

    std::atomic<int> threaded_lines{0}, threaded_tails{0};
    std::vector<std::thread> threads;
    for (int t = 0; t < 8; t++)
        threads.emplace_back([&] {
            for (int i = 0; i < 200; i++)
                if (auto line = threaded.note("vehicle_" + std::to_string(i % 50))) {
                    threaded_lines++;
                    threaded_tails += *line.tail != 0;
                }
        });
    for (auto &t : threads)
        t.join();
    expect(threaded_lines.load() == 32 && threaded_tails.load() == 1 && threaded.size() == 32,
           "eight threads noting fifty keys log exactly the first thirty-two once, one with the tail");

    guards::FirstBytes tracks;
    expect(tracks.first(0) && tracks.first(255), "the first note of keys 0 and 255 is new");
    expect(!tracks.first(0) && !tracks.first(255), "a key noted before is not new");
    int fresh = 0;
    for (int i = 0; i < 256; i++)
        fresh += tracks.first(uint8_t(i));
    expect(fresh == 254, "every other one-byte key is new once, with no cap");

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
