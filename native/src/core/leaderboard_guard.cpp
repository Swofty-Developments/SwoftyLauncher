#include "runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

// Time Trial leaderboard guard (re/db/functions.tsv Game_TimeTrialLeaderboard_SelectBoard).
//
// Pressing View Leaderboards in Time Trial runs Game_TimeTrialLeaderboard_SelectBoard through the
// leaderboard_container binding. It builds the board key 'LBV%02u%02d%s' from the track and the weather, then
// searches the leaderboard list (self+0x478 -> +0x10 -> +0x18..+0x20) for the board whose name (+0x68) equals the
// key, and reads the board's rank thresholds (+0x88, +0x98..+0xa0). The search leaves 0 in rcx when no board
// carries the key, and the read has no null check: with the online services off (the list is empty) or at a track
// without a board, the first read faults (ACCESS_VIOLATION reading 0x98 at F1_25.exe+0x2fce9e3).
//
// site_Game_TimeTrialLeaderboard_BoardRead is that read (rcx = the board). When the board is null the guard sets
// esi to 0, the function's return value, and continues at site_Game_TimeTrialLeaderboard_BoardsDone, the function's
// own path after the board work, which releases its locals and returns esi. The caller that tests the result
// takes its failure branch, and the other caller ignores it. A board that was found runs the game's code
// unchanged.
//
// SWOFTY_LEADERBOARD_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (time trial leaderboard guard)";
constexpr int LOG_FIRST = 4;

uintptr_t boards_done = 0;
bool offered = false, settled = false;
std::atomic<int> skipped{0};

void board_read(SwoftyContext *c)
{
    if (c->rcx)
        return;
    c->rsi = 0;
    c->rip = boards_done;
    int n = ++skipped;
    if (n > LOG_FIRST)
        return;
    swofty::log("[leaderboard] Time Trial leaderboard view 0x%llx has no board for this track (online services off or no "
                "board listed); view skipped%s",
                (unsigned long long)c->rdi, n == LOG_FIRST ? " (further skips are not logged)" : "");
}

}

namespace runtime {

std::vector<CoreMixins> leaderboard_guard_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_LEADERBOARD_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[leaderboard] time trial leaderboard guard off (SWOFTY_LEADERBOARD_GUARD=%s)", v);
        return out;
    }
    boards_done = swofty::api->resolve("site_Game_TimeTrialLeaderboard_BoardsDone");
    if (!boards_done || !swofty::api->resolve("site_Game_TimeTrialLeaderboard_BoardRead")) {
        boards_done = 0;
        swofty::log("[leaderboard] time trial leaderboard guard off: needs site_Game_TimeTrialLeaderboard_BoardRead "
                    "and site_Game_TimeTrialLeaderboard_BoardsDone, which this build does not resolve");
        return out;
    }
    out.push_back({MIXIN,
                   {{"site_Game_TimeTrialLeaderboard_BoardRead", SWOFTY_MIXIN_SITE, 100, (void *)&board_read, nullptr}}});
    return out;
}

void leaderboard_guard_commit()
{
    if (!offered || settled || !boards_done)
        return;
    settled = true;
    swofty::log("[leaderboard] time trial leaderboard guard %s",
                core_mixins_active(MIXIN)
                    ? "armed: View Leaderboards at a track without a board returns instead of faulting"
                    : "did not install (see the [core] lines)");
}

}
