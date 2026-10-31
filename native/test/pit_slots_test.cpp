// Host-native unit test for the core's pit slot guard (native/src/core/pit_slots_logic.hpp): which slot pointers
// count as usable and how the capped, per-vehicle log line is spaced out.
// Built and run by native/test/run_pit_slots_test.sh.

#include "pit_slots_logic.hpp"

#include <cstdio>

namespace ps = runtime::pit_slots;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    expect(!ps::usable(0), "a null slot is not usable");
    expect(!ps::usable(0x48), "a small offset read as a pointer is not usable");
    expect(ps::usable(0x7ff612340000), "an image-range pointer is usable");
    expect(ps::usable(ps::LOWEST_POINTER), "the lowest accepted pointer is usable");

    ps::Seen seen;
    expect(seen.note(ps::Site::eval_list, 21) == ps::Log::line, "the first vehicle is logged");
    expect(seen.note(ps::Site::eval_list, 21) == ps::Log::none, "the same vehicle at the same site is logged once");
    expect(seen.note(ps::Site::rank_list, 21) == ps::Log::line, "the same vehicle at another site is logged");
    expect(seen.note(ps::Site::eval_own, 21) == ps::Log::line, "the own-car site is keyed apart from the list site");
    for (int v = 0; v < 4; v++)
        seen.note(ps::Site::eval_list, uint8_t(100 + v));
    expect(seen.logged == 7, "seven distinct reports so far");
    expect(seen.note(ps::Site::rank_own, 3) == ps::Log::last, "the eighth report is marked as the last");
    expect(seen.note(ps::Site::rank_own, 4) == ps::Log::none, "nothing is logged past the cap");
    expect(seen.note(ps::Site::eval_list, 21) == ps::Log::none, "a repeat after the cap stays quiet");

    ps::Seen many;
    int lines = 0;
    for (int v = 0; v < 200; v++)
        if (many.note(ps::Site::eval_list, uint8_t(v)) != ps::Log::none)
            lines++;
    expect(lines == ps::LOG_FIRST, "two hundred vehicles produce exactly the capped number of lines");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
