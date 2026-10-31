// Host-native unit test for the core's tyre slot guard (native/src/core/tyre_slots_logic.hpp): which slot the
// game would pick for a fitted set's option, and what the guard does when that pick is missing or unusable.
// Built and run by native/test/run_tyre_slots_test.sh.

#include "tyre_slots_logic.hpp"

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

namespace ts = runtime::tyre_slots;

static ts::Slot slot(uint32_t option, uint64_t physical = 0x1000, uint64_t visual = 0x2000)
{
    return {option, physical ? physical + option : 0, visual ? visual + option : 0};
}

static ts::Plan plan(const std::vector<ts::Slot> &t, uint32_t option)
{
    return ts::plan(t.data(), t.size(), option);
}

int main()
{
    CHECK(ts::fallback(0) == 1 && ts::fallback(1) == 1 && ts::fallback(2) == 1, "dry options fall back to Medium");
    CHECK(ts::fallback(3) == 4 && ts::fallback(4) == 4 && ts::fallback(7) == 4,
          "Intermediate, Wet and higher fall back to Wet");

    std::vector<ts::Slot> f1 = {slot(0), slot(1), slot(2), slot(3), slot(4)};
    for (uint32_t o = 0; o < 5; o++) {
        auto p = plan(f1, o);
        CHECK(p.kind == ts::Kind::keep && p.index == int(o), "an F1 class table keeps every option's own slot");
    }

    std::vector<ts::Slot> f2 = {slot(1), slot(2), slot(4)};
    auto p = plan(f2, 0);
    CHECK(p.kind == ts::Kind::keep && p.index == 0, "an F2 table serves Softer through its Medium slot");
    p = plan(f2, 3);
    CHECK(p.kind == ts::Kind::keep && p.index == 2, "an F2 table serves Intermediate through its Wet slot");

    std::vector<ts::Slot> wet_only = {slot(4)};
    for (uint32_t o = 0; o < 3; o++) {
        p = plan(wet_only, o);
        CHECK(p.kind == ts::Kind::add && p.index == 1 && p.donor == 0,
              "a dry set on a table holding only Wet gets a slot added from the Wet slot");
    }
    p = plan(wet_only, 3);
    CHECK(p.kind == ts::Kind::keep && p.index == 0, "an Intermediate set on that table keeps the Wet slot");

    std::vector<ts::Slot> mixed = {slot(4), slot(3), slot(2)};
    p = plan(mixed, 0);
    CHECK(p.kind == ts::Kind::add && p.index == 3 && p.donor == 2, "a missing dry option prefers a dry donor");
    std::vector<ts::Slot> dry = {slot(0), slot(2)};
    p = plan(dry, 4);
    CHECK(p.kind == ts::Kind::add && p.index == 2 && p.donor == 0, "a missing wet option takes a dry donor when no wet slot");

    std::vector<ts::Slot> nulls = {slot(1, 0), slot(2)};
    p = plan(nulls, 0);
    CHECK(p.kind == ts::Kind::fill && p.index == 0 && p.donor == 1,
          "a pick without a physical compound is filled from a usable slot");
    std::vector<ts::Slot> novis = {slot(0, 0x1000, 0), slot(4)};
    p = plan(novis, 0);
    CHECK(p.kind == ts::Kind::fill && p.index == 0 && p.donor == 1, "a pick without a visual compound is filled too");

    std::vector<ts::Slot> empty;
    p = plan(empty, 0);
    CHECK(p.kind == ts::Kind::skip, "an empty table skips the refit");
    std::vector<ts::Slot> dead = {slot(1, 0), slot(4, 0x1000, 0)};
    p = plan(dead, 0);
    CHECK(p.kind == ts::Kind::skip, "a table with no usable slot skips the refit");

    std::vector<ts::Slot> full = {slot(0), slot(1), slot(2), slot(3), slot(5)};
    p = plan(full, 6);
    CHECK(p.kind == ts::Kind::skip, "a full table without the option or its fallback skips the refit");
    std::vector<ts::Slot> holed = {slot(0), slot(1), slot(2), slot(3), slot(5)};
    p = plan(holed, 4);
    CHECK(p.kind == ts::Kind::skip, "a full table missing Wet skips a Wet set");

    ts::Slot six[6] = {slot(0), slot(1), slot(2), slot(3), slot(4), slot(4)};
    p = ts::plan(six, 6, 0);
    CHECK(p.kind == ts::Kind::skip && ts::find(six, 6, 0) == -1, "a count past the 5 inline slots is never trusted");

    if (failures) {
        printf("%d failure(s)\n", failures);
        return 1;
    }
    printf("all tyre slot checks passed\n");
    return 0;
}
