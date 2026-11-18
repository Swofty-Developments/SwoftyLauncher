// Host-native unit test for the ERS mode guard's per-build mode offset (native/src/core/ers_mode_logic.hpp): +0x1a68
// on the KB build, +0x1a6c on Steam 1.0.145, and no offset on any build without a witness.
// Built and run by native/test/run_ers_mode_test.sh.

#include "ers_mode_logic.hpp"

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

namespace ers = runtime::ers_mode;

static_assert(ers::mode_offset(1789118616u, 0x6a00000) == 0x1a68);
static_assert(ers::mode_offset(1794312636u, 0x18434000) == 0x1a6c);

int main()
{
    CHECK(ers::mode_offset(1789118616u, 0x6a00000) == 0x1a68, "the KB build reads the mode at +0x1a68");
    CHECK(ers::mode_offset(1789118616u, 0x18000000) == 0x1a68, "the KB build's unpacked image reads it at +0x1a68");
    CHECK(ers::mode_offset(1789118616u, 0x6000000) == 0,
          "the KB timestamp with an image no larger than 0x6000000 is not the KB build");
    CHECK(ers::mode_offset(1794312636u, 0x18434000) == 0x1a6c, "Steam 1.0.145 reads the mode at +0x1a6c");
    CHECK(ers::mode_offset(1794312636u, 0x18435000) == 0, "the 1.0.145 timestamp with another image size has no offset");
    CHECK(ers::mode_offset(1789128892u, 0x155dc000) == 0, "the EA App KB build has no witness, so no offset");
    CHECK(ers::mode_offset(1794315435u, 0x1615e000) == 0, "EA 1.0.145 has no witness, so no offset");
    CHECK(ers::mode_offset(1799999999u, 0x18500000) == 0 && ers::mode_offset(0, 0) == 0, "an unknown build has no offset");

    printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
