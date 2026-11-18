// Host-native unit test for the track acclimatisation guards' decision (native/src/core/guards/tracks/track_acclim_logic.hpp):
// a programme skips its refresh and its penalty flag only when its target vector reads and is empty.
// Built and run by native/test/run_tests.sh track_acclim.

#include "core/guards/tracks/track_acclim_logic.hpp"

#include <cstdio>
#include <cstring>

static int failures;

#define CHECK(cond, msg)                                                                                                         \
    do {                                                                                                                         \
        if (cond) {                                                                                                              \
            printf("ok  : %s\n", msg);                                                                                           \
        } else {                                                                                                                 \
            printf("FAIL: %s\n", msg);                                                                                           \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

namespace ta = runtime::track_acclim;

struct Programme {
    uint8_t bytes[0x360] = {};
    size_t readable = sizeof bytes;
};

static bool read_programme(uint64_t address, void *out, size_t n, void *user)
{
    auto *p = static_cast<Programme *>(user);
    if (address + n > p->readable)
        return false;
    memcpy(out, p->bytes + address, n);
    return true;
}

static void targets(Programme &p, uint64_t begin, uint64_t end)
{
    memcpy(p.bytes + ta::TARGETS_BEGIN, &begin, sizeof begin);
    memcpy(p.bytes + ta::TARGETS_END, &end, sizeof end);
}

int main()
{
    Programme p;
    uint64_t begin = 1;
    CHECK(ta::no_targets(0, read_programme, &p, begin) && begin == 0,
          "a track with no CornerData leaves the vector null, and the programme is skipped");

    targets(p, 0x20000, 0x20000);
    CHECK(ta::no_targets(0, read_programme, &p, begin) && begin == 0x20000,
          "an empty vector that has storage is skipped too, and the line gets its start");

    targets(p, 0x20000, 0x20000 + 0x370);
    CHECK(!ta::no_targets(0, read_programme, &p, begin), "a programme with one target runs the game's code");

    targets(p, 0x20000, 0x20000 + 12 * 0x370);
    CHECK(!ta::no_targets(0, read_programme, &p, begin), "a programme with a stock track's targets runs the game's code");

    targets(p, 0x20000 + 0x370, 0x20000);
    CHECK(!ta::no_targets(0, read_programme, &p, begin), "a vector whose ends differ is never read as empty");

    targets(p, 0, 0);
    p.readable = ta::TARGETS_END;
    CHECK(!ta::no_targets(0, read_programme, &p, begin), "a programme whose vector end can't be read runs the game's code");
    p.readable = ta::TARGETS_BEGIN;
    CHECK(!ta::no_targets(0, read_programme, &p, begin), "a programme whose vector start can't be read runs the game's code");

    if (failures) {
        printf("%d failure(s)\n", failures);
        return 1;
    }
    printf("all track acclimatisation checks passed\n");
    return 0;
}
