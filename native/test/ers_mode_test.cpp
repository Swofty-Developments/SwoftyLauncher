// Host-native unit test for the ERS mode guard's per-build mode offset (native/src/core/guards/cars/ers_mode_logic.hpp):
// +0x1a68 on the KB build, +0x1a6c on both 1.0.145 builds, and no offset on a build without a witness.
// Built and run by native/test/run_tests.sh ers_mode.

#include "core/guards/cars/ers_mode_logic.hpp"

#include <cstdio>

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

namespace bp = runtime::build_proof;
namespace ers = runtime::ers_mode;

static_assert(ers::mode_offset(bp::Build::kb) == 0x1a68 && ers::mode_offset(bp::Build::steam_1_0_145) == 0x1a6c);

int main()
{
    CHECK(ers::mode_offset(bp::identify({1789118616u, 0x6a00000})) == 0x1a68, "the KB build reads the mode at +0x1a68");
    CHECK(ers::mode_offset(bp::identify({1794312636u, 0x18434000})) == 0x1a6c, "Steam 1.0.145 reads the mode at +0x1a6c");
    CHECK(ers::mode_offset(bp::identify({1794315435u, 0x1615e000})) == 0x1a6c, "EA 1.0.145 reads the mode at +0x1a6c");
    CHECK(ers::mode_offset(bp::identify({1794312636u, 0x18435000})) == 0,
          "the 1.0.145 timestamp with another image size has no offset");
    CHECK(ers::mode_offset(bp::Build::ea) == 0, "the EA App build has no witness, so no offset");
    CHECK(ers::mode_offset(bp::Build::unknown) == 0 && ers::mode_offset(bp::identify({1799999999u, 0x18500000})) == 0,
          "an unknown build has no offset");
    int known = 0;
    for (const bp::Known &k : bp::KNOWN)
        known += ers::mode_offset(k.build) != 0;
    CHECK(known == 3, "three of the known builds have a witness for the mode's offset");
    uint32_t seen = 0;
    bool twice = false;
    for (const runtime::car_layout::PerBuild &m : runtime::car_layout::ERS_MODE) {
        twice |= (seen & m.proof.builds) != 0;
        seen |= m.proof.builds;
    }
    CHECK(!twice, "no build has two mode offsets");
    for (const bp::Known &k : bp::KNOWN)
        CHECK(ers::mode_offset(k.build) == runtime::car_layout::ers_mode(k.build) || !ers::mode_offset(k.build),
              "the guard's offset is the shared car layout's");

    printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
