#include "replay_grid_logic.hpp"

#include <cstdio>
#include <thread>
#include <vector>

static int failures;

#define CHECK(cond, msg)                                                                                                \
    do {                                                                                                                \
        if (cond) {                                                                                                     \
            printf("ok  : %s\n", msg);                                                                                  \
        } else {                                                                                                        \
            printf("FAIL: %s\n", msg);                                                                                  \
            failures++;                                                                                                 \
        }                                                                                                               \
    } while (0)

int main()
{
    namespace rg = runtime::replay_grid;
    const uint64_t base = 0x146cc4ec0;
    CHECK(!rg::outgrew(base, base, 0x1c00), "an empty state is recorded");
    CHECK(!rg::outgrew(base + 0x1c00, base, 0x1c00), "a state exactly the holder's capacity is recorded");
    CHECK(rg::outgrew(base + 0x1c01, base, 0x1c00), "a state one byte over the holder's capacity is skipped");
    CHECK(rg::outgrew(base + 0x4000, base, 0x20), "a small holder with a large state is skipped");
    CHECK(!rg::outgrew(base + 0x4000, base, 0), "a capacity that read as zero leaves the game's code alone");
    CHECK(!rg::outgrew(base - 8, base, 0x20), "a position before the base leaves the game's code alone");
    CHECK(rg::capacity_of(0xdead00000020ull) == 0x20, "the capacity is the low 32 bits of the method's return");
    CHECK(!rg::guards(22) && !rg::guards(24), "a stock grid of up to 24 cars runs the game's code unchanged");
    CHECK(rg::guards(25) && rg::guards(30), "a grid past 24 cars is guarded");
    CHECK(rg::guards(-1), "an unknown car count is guarded");
    CHECK(!rg::outgrew(base + 0x20, base, rg::capacity_of(0xdead00000020ull)), "stale high bits do not shrink the capacity");

    std::string l = rg::line(25, "VehicleSystem", 4096, 3072);
    CHECK(l == "[replay] session has 25 cars, the recorder holds 22; VehicleSystem needs 4096 bytes of replay state and its "
               "holder has 3072, so it is not recorded (replay and flashback lose its state; the session keeps all its cars)",
          "the log line names the cars, the recorder's capacity and the recordable");
    CHECK(rg::line(-1, "", 8, 4).find("session has ? cars") != std::string::npos, "an unknown car count reads as ?");
    CHECK(rg::line(-1, "", 8, 4).find("; ? needs 8 bytes") != std::string::npos, "an unnamed recordable reads as ?");

    rg::Tally tally;
    CHECK(tally.first(), "the first skip is logged");
    CHECK(!tally.first() && !tally.first(), "later skips are not logged");
    CHECK(tally.skipped.load() == 3, "every skip is counted");

    rg::Tally shared;
    std::atomic<int> logged{0};
    std::vector<std::thread> workers;
    for (int i = 0; i < 8; i++)
        workers.emplace_back([&] {
            for (int k = 0; k < 1000; k++)
                if (shared.first())
                    logged++;
        });
    for (auto &w : workers)
        w.join();
    CHECK(logged == 1, "threads log one line between them");
    return failures ? 1 : 0;
}
