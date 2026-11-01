#include "track_record_logic.hpp"

#include <cstdio>

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
    runtime::track_record::Seen seen;
    CHECK(seen.first(1), "the first miss of a track is logged");
    CHECK(!seen.first(1), "the same track again is not logged");
    CHECK(seen.first(18), "another track is logged");
    CHECK(seen.first(255) && !seen.first(255), "track 255 is logged once");
    CHECK(seen.first(0), "track 0 is logged");
    return failures ? 1 : 0;
}
