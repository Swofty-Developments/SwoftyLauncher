// Host-native unit test for the core's Thrustmaster LCD guard (native/src/core/thrustmaster_lcd_logic.hpp).
// Built and run by native/test/run_thrustmaster_lcd_test.sh.

#include "thrustmaster_lcd_logic.hpp"

#include <cstdio>

namespace tl = runtime::thrustmaster_lcd;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    expect(tl::record_missing(0), "a null entrant record is skipped");
    expect(!tl::record_missing(0x325a308a0), "a found entrant record runs the game's code");
    expect(!tl::record_missing(1), "any non-null record is kept");

    tl::Skips s;
    int lines = 0, last = 0;
    for (int i = 0; i < 500; i++) {
        int n = s.note();
        if (s.logged(n))
            lines++;
        if (s.last(n))
            last++;
    }
    expect(lines == tl::LOG_FIRST, "five hundred skips produce exactly the capped number of lines");
    expect(last == 1, "the capped line is marked once");
    expect(s.count == 500, "every skip is counted");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
