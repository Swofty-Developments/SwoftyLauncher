// Host-native unit test for the participant step of the core's ERS new-lap site (native/src/core/ers_newlap_logic.hpp):
// which entries the guard skips, and the one entry per vehicle_newlap message where session.lap fires.
// Built and run by native/test/run_ers_newlap_test.sh.

#include "ers_newlap_logic.hpp"

#include <cstdio>
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

namespace ers = runtime::ers;

struct Participant {
    uint64_t entry, vehicle;
    int id;
};

static std::vector<ers::Step> walk(const std::vector<Participant> &list, uint8_t car, int *laps)
{
    std::vector<ers::Step> seen;
    *laps = 0;
    for (auto &p : list) {
        auto s = ers::step(p.entry, p.vehicle, p.id, car);
        seen.push_back(s);
        if (s == ers::Step::lap) {
            ++*laps;
            break;
        }
    }
    return seen;
}

int main()
{
    CHECK(ers::step(0, 0x1000, 3, 3) == ers::Step::skip, "a null participant entry is skipped");
    CHECK(ers::step(0x2000, 0, 3, 3) == ers::Step::skip, "a participant without a vehicle component is skipped");
    CHECK(ers::step(0x2000, 0x1000, -1, 3) == ers::Step::skip, "an unreadable vehicle id is skipped");
    CHECK(ers::step(0x2000, 0x1000, 4, 3) == ers::Step::other, "another car's participant runs the game's compare");
    CHECK(ers::step(0x2000, 0x1000, 3, 3) == ers::Step::lap, "the message's car fires session.lap");
    CHECK(ers::step(0x2000, 0x1000, 0x103, 3) == ers::Step::lap, "the vehicle id compares as a byte, as the game's cmp does");

    std::vector<Participant> tt = {{0x10, 0, 0}, {0x20, 0, 0}, {0x30, 0x300, 0}};
    int laps = 0;
    auto steps = walk(tt, 0, &laps);
    CHECK(laps == 1 && steps.size() == 3 && steps[0] == ers::Step::skip && steps[1] == ers::Step::skip,
          "Time Trial: participants without a vehicle ahead of the player's car are skipped, the player's lap fires once");

    std::vector<Participant> grid;
    for (int i = 0; i < 26; i++)
        grid.push_back({uint64_t(0x100 + i), uint64_t(0x1000 + i), i});
    steps = walk(grid, 17, &laps);
    CHECK(laps == 1 && steps.size() == 18, "a 26-car session: one session.lap for car 17, at its own participant");
    steps = walk(grid, 30, &laps);
    CHECK(laps == 0 && steps.size() == 26, "a vehicle id no participant has fires nothing");

    printf("%s\n", failures ? "FAILED" : "all ers new-lap step checks passed");
    return failures ? 1 : 0;
}
