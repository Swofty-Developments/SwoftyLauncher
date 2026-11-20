// Host unit test for the exit hooks' crash report decision (exit_logic.hpp). native/test/run_tests.sh exit_logic builds
// and runs it.
#include "core/diagnostics/crash/exit_logic.hpp"

#include <cstdio>
#include <string>

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

using exit_logic::Exit;
using exit_logic::Verdict;
using exit_logic::verdict;

int main()
{
    const uint64_t now = 500000;
    expect(verdict(Exit{0, false, now, 0, false}) == Verdict::Clean, "a clean exit gets no report");
    expect(verdict(Exit{1, false, now, 0, false}) == Verdict::Crash, "TerminateProcess(1) with no close asked for gets a report");
    expect(verdict(Exit{0xFFFFFFFF, false, now, 0, false}) == Verdict::Crash, "the game's own -1 gets a report");
    expect(verdict(Exit{0, true, now, 0, false}) == Verdict::Crash,
           "a clean exit just after a fatal-class exception gets a report");

    expect(verdict(Exit{1, false, now, 0, true}) == Verdict::Close,
           "TerminateProcess(1) after the launcher's Stop or its restart of the game is a close, not a crash");
    expect(verdict(Exit{1, false, now, now - 3000, false}) == Verdict::Close,
           "TerminateProcess(1) 3 s after the window got WM_CLOSE is a close, not a crash");
    expect(verdict(Exit{1, false, now, now, false}) == Verdict::Close, "an exit in the same tick as WM_CLOSE is a close");
    expect(verdict(Exit{1, false, now, now - exit_logic::CLOSE_MS + 1, false}) == Verdict::Close,
           "an exit just inside the close window is a close");
    expect(verdict(Exit{1, false, now, now - exit_logic::CLOSE_MS, false}) == Verdict::Crash,
           "an exit a minute after a close the game ignored gets a report");
    expect(verdict(Exit{1, false, now, now + 10, false}) == Verdict::Crash,
           "a close time after the exit is not a close before it");
    expect(verdict(Exit{0, false, now, now - 1000, true}) == Verdict::Clean, "a clean exit after a close stays clean");

    expect(verdict(Exit{1, true, now, now - 1000, true}) == Verdict::Crash,
           "an exit just after a fatal-class exception gets a report even when a close was asked for");
    expect(verdict(Exit{0xC0000005, true, now, 0, true}) == Verdict::Crash,
           "a crash while the launcher stops the game still gets a report");

    expect(exit_logic::stop_file("C:\\Users\\id\\AppData\\Local\\Swofty\\sessions\\20261117-013735") ==
               "C:\\Users\\id\\AppData\\Local\\Swofty\\sessions\\20261117-013735\\stop_requested",
           "the stop request is the stop_requested file in the session latest.txt names");
    expect(exit_logic::stop_file("  D:\\Swofty\\sessions\\20261120-101500\\\r\n") ==
               "D:\\Swofty\\sessions\\20261120-101500\\stop_requested",
           "blanks around latest.txt's path and a trailing separator are dropped");
    expect(exit_logic::stop_file("").empty(), "no session named, no stop request file");
    expect(exit_logic::stop_file(" \r\n").empty(), "a blank latest.txt names no session");

    const uint64_t started = 134080000000000000ull;
    expect(exit_logic::stop_is_ours(started + 10'000'000, started), "a stop written after the game started is its own");
    expect(exit_logic::stop_is_ours(started, started), "a stop written as the game started is its own");
    expect(!exit_logic::stop_is_ours(started - 1, started), "a stop written before the game started is an earlier session's");
    expect(!exit_logic::stop_is_ours(0, started), "a stop file with no time is not taken");
    return fails ? 1 : 0;
}
