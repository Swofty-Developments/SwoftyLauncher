// Host-native unit test for the core's Time Trial leaderboard guards (native/src/core/leaderboard_guard_logic.hpp):
// when a missing board or child list model is skipped, how often the skip is logged, and how the env switches parse.
// Built and run by native/test/run_leaderboard_guard_test.sh.

#include "leaderboard_guard_logic.hpp"

#include <cstdio>

namespace lg = runtime::leaderboard_guard;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    expect(lg::board_missing(0), "a null board is skipped");
    expect(!lg::board_missing(0x32b702d80), "a board that was found runs the game's code");
    expect(lg::tab_child_missing(0), "a null child list model skips the tab activation");
    expect(!lg::tab_child_missing(0x2e9607f98), "a child list model that exists runs the game's code");

    expect(lg::say(0) == lg::Say::none, "occurrence 0 is not logged");
    expect(lg::say(1) == lg::Say::line, "the first skip is logged");
    expect(lg::say(lg::LOG_FIRST - 1) == lg::Say::line, "skips before the cap are logged plainly");
    expect(lg::say(lg::LOG_FIRST) == lg::Say::last, "the last logged skip says the rest are silent");
    expect(lg::say(lg::LOG_FIRST + 1) == lg::Say::none, "skips past the cap are silent");
    expect(lg::say(1000000) == lg::Say::none, "a long run stays silent");

    expect(lg::switch_off("0"), "SWOFTY_LEADERBOARD_TAB_GUARD=0 turns it off");
    expect(lg::switch_off("off"), "SWOFTY_LEADERBOARD_TAB_GUARD=off turns it off");
    expect(!lg::switch_off("1"), "1 keeps it on");
    expect(!lg::switch_off("00"), "00 keeps it on");
    expect(!lg::switch_off("offer"), "offer keeps it on");
    expect(!lg::switch_off(""), "an empty value keeps it on");
    expect(!lg::switch_off(nullptr), "an unset variable keeps it on");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
