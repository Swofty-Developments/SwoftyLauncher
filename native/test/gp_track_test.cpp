// Host-native unit test for the Grand Prix remembered-track fallback (native/src/core/apply/gp_track_logic.hpp): a
// weekend track stat id with no tracks row is swapped for the stock track, one with a row (stock, addon or dormant)
// is left to the era guard, and an unreadable table or a missing stock row changes nothing. Built and run by
// native/test/run_gp_track_test.sh.

#include "apply/gp_track_logic.hpp"

#include <cstdio>

namespace gp = swofty_core::apply::gp_track;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    const std::map<int, std::string> rows{{0, "melbourne"}, {1, "shanghai"}, {2, "suzuka"}, {3, "bahrain"}, {45, "adelaide"}};

    gp::Fallback f = gp::fallback(52, rows);
    expect(f.replace && f.stat_id == 0, "stat id 52 with no tracks row is replaced by melbourne (stat id 0)");
    expect(f.line.find("stat id 52") != std::string::npos && f.line.find("melbourne (stat id 0)") != std::string::npos,
           "the line names the stale id and the stock track");

    f = gp::fallback(83, rows);
    expect(f.replace && f.stat_id == 0, "stat id 83 (an addon turned off) is replaced");

    f = gp::fallback(35, rows);
    expect(f.replace && f.stat_id == 0, "stat id 35 (a track of another profile) is replaced");

    f = gp::fallback(3, rows);
    expect(!f.replace && f.line.empty(), "a stock track with a row is left alone");

    f = gp::fallback(45, rows);
    expect(!f.replace && f.line.empty(), "an addon track with a row is left alone (the era guard owns dormant rows)");

    f = gp::fallback(-1, rows);
    expect(!f.replace && f.line.empty(), "no weekend track (byte 0xff) changes nothing");

    f = gp::fallback(0xff, rows);
    expect(!f.replace && f.line.empty(), "stat id 0xff changes nothing");

    f = gp::fallback(52, {});
    expect(!f.replace && f.line.empty(), "an unreadable tracks table changes nothing and says nothing");

    const std::map<int, std::string> no_stock{{1, "shanghai"}, {2, "suzuka"}};
    f = gp::fallback(52, no_stock);
    expect(!f.replace && f.stat_id == -1 && f.line.find("no melbourne row") != std::string::npos,
           "no stock row: nothing written, the line says why");

    const std::map<int, std::string> moved{{7, "melbourne"}, {8, "shanghai"}};
    f = gp::fallback(52, moved);
    expect(f.replace && f.stat_id == 7, "the stock track's stat id comes from the table, not a constant");

    f = gp::fallback(52, rows, "suzuka");
    expect(f.replace && f.stat_id == 2, "another stock label is honoured");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
