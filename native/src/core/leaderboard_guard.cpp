#include "leaderboard_guard_logic.hpp"
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
// Activating the leaderboard tab runs Sess_TimeTrialLeaderboard_ActivateTab (vtable slot 0x13 of the same model), which
// reads the child list model at self+0x9a0 and calls through it with no null check (F1_25.exe+0x2fcbb09 reading 0). The
// child is null when the screen opened for a track the view skipped above. site_Sess_TimeTrialLeaderboard_TabChild is
// that read (rcx = the child). When it is null the guard continues at site_Sess_TimeTrialLeaderboard_TabDone, the
// function's own epilogue, which skips the child call and the refresh that follows it. A child that exists runs the
// game's code unchanged.
//
// Entering Time Trial (setCurrentGameMode none -> time_trial) runs Game_TimeTrial_SetReference, which stores the board
// for the reference's track and weather at presenter+0x20278 and then, when g_Ne_LogGate holds the GAME channel, logs
// '[TIME TRIALS] New reference : ... leaderboard(%s).' with the board's name read from [board+0x40]. The board is 0 when
// no board is listed for the track (offline play has no online board list), and the read faults (ACCESS_VIOLATION
// reading 0x40 at F1_25.exe+0x147bc9b). Requesting a rival ghost from the board logs the same name on GHOSTLAP-TRACE in
// Game_GhostlapTrace_OnRivalFromLeaderboardDownloadRequested. Stock builds keep both channels closed; swofty_gamelog
// opens them for a GAME or GHOSTLAP-TRACE channel rule. site_Game_TimeTrial_SetReferenceLogBoard and
// site_Game_GhostlapTrace_RivalLogBoard are those reads (rsi = the presenter). For a null board the guard continues at
// site_Game_TimeTrial_SetReferenceLogDone or site_Game_GhostlapTrace_RivalLogDone, the paths the game takes with the
// channel closed, so only the log line is skipped. A board that was found is logged as the game does.
//
// SWOFTY_LEADERBOARD_GUARD=0 turns the board guard off, SWOFTY_LEADERBOARD_TAB_GUARD=0 the tab guard,
// SWOFTY_TT_REFERENCE_LOG_GUARD=0 the reference log guard.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (time trial leaderboard guard)";
namespace lg = runtime::leaderboard_guard;

constexpr const char *TAB_MIXIN = "swofty_core.dll (time trial leaderboard tab guard)";
constexpr const char *REFERENCE_MIXIN = "swofty_core.dll (time trial reference log guard)";
constexpr uintptr_t PRESENTER_BOARD = 0x20278;

uintptr_t boards_done = 0, tab_done = 0, reference_done = 0, rival_done = 0;
bool offered = false, settled = false;
std::atomic<int> skipped{0}, tab_skipped{0}, reference_skipped{0};

bool env_off(const char *name, const char *label)
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    if (!n || n >= sizeof v || !lg::switch_off(v))
        return false;
    swofty::log("[leaderboard] %s off (%s=%s)", label, name, v);
    return true;
}

void board_read(SwoftyContext *c)
{
    if (!lg::board_missing(c->rcx))
        return;
    c->rsi = 0;
    c->rip = boards_done;
    lg::Say say = lg::say(++skipped);
    if (say == lg::Say::none)
        return;
    swofty::log("[leaderboard] Time Trial leaderboard view 0x%llx has no board for this track (online services off or no "
                "board listed); view skipped%s",
                (unsigned long long)c->rdi, say == lg::Say::last ? " (further skips are not logged)" : "");
}

void tab_child_read(SwoftyContext *c)
{
    if (!lg::tab_child_missing(c->rcx))
        return;
    c->rip = tab_done;
    lg::Say say = lg::say(++tab_skipped);
    if (say == lg::Say::none)
        return;
    swofty::log("[leaderboard] Time Trial leaderboard model 0x%llx activated its tab with no child list model; tab "
                "activation skipped%s",
                (unsigned long long)(c->rsi - 0x660), say == lg::Say::last ? " (further skips are not logged)" : "");
}

bool presenter_board_missing(uintptr_t presenter)
{
    uint64_t board = 0;
    return !swofty::mem::read(presenter + PRESENTER_BOARD, board) || lg::reference_board_missing(board);
}

void reference_log_skipped(uintptr_t presenter, const char *line)
{
    lg::Say say = lg::say(++reference_skipped);
    if (say == lg::Say::none)
        return;
    swofty::log("[leaderboard] Time Trial presenter 0x%llx has no board for this reference (online services off or no "
                "board listed); the game's %s log line skipped%s",
                (unsigned long long)presenter, line, say == lg::Say::last ? " (further skips are not logged)" : "");
}

void reference_log_board(SwoftyContext *c)
{
    if (!presenter_board_missing(c->rsi))
        return;
    c->rip = reference_done;
    reference_log_skipped(c->rsi, "GAME 'New reference'");
}

void rival_log_board(SwoftyContext *c)
{
    if (!presenter_board_missing(c->rsi))
        return;
    c->rip = rival_done;
    reference_log_skipped(c->rsi, "GHOSTLAP-TRACE 'Ghost requested'");
}

}

namespace runtime {

std::vector<CoreMixins> leaderboard_guard_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    if (!env_off("SWOFTY_LEADERBOARD_GUARD", "time trial leaderboard guard")) {
        boards_done = swofty::api->resolve("site_Game_TimeTrialLeaderboard_BoardsDone");
        if (!boards_done || !swofty::api->resolve("site_Game_TimeTrialLeaderboard_BoardRead")) {
            boards_done = 0;
            swofty::log("[leaderboard] time trial leaderboard guard off: needs site_Game_TimeTrialLeaderboard_BoardRead "
                        "and site_Game_TimeTrialLeaderboard_BoardsDone, which this build does not resolve");
        } else {
            out.push_back({MIXIN,
                           {{"site_Game_TimeTrialLeaderboard_BoardRead", SWOFTY_MIXIN_SITE, 100, (void *)&board_read, nullptr}}});
        }
    }
    if (!env_off("SWOFTY_LEADERBOARD_TAB_GUARD", "time trial leaderboard tab guard")) {
        tab_done = swofty::api->resolve("site_Sess_TimeTrialLeaderboard_TabDone");
        if (!tab_done || !swofty::api->resolve("site_Sess_TimeTrialLeaderboard_TabChild")) {
            tab_done = 0;
            swofty::log("[leaderboard] time trial leaderboard tab guard off: needs site_Sess_TimeTrialLeaderboard_TabChild "
                        "and site_Sess_TimeTrialLeaderboard_TabDone, which this build does not resolve");
        } else {
            out.push_back({TAB_MIXIN,
                           {{"site_Sess_TimeTrialLeaderboard_TabChild", SWOFTY_MIXIN_SITE, 100, (void *)&tab_child_read, nullptr}}});
        }
    }
    if (!env_off("SWOFTY_TT_REFERENCE_LOG_GUARD", "time trial reference log guard")) {
        reference_done = swofty::api->resolve("site_Game_TimeTrial_SetReferenceLogDone");
        rival_done = swofty::api->resolve("site_Game_GhostlapTrace_RivalLogDone");
        if (!reference_done || !swofty::api->resolve("site_Game_TimeTrial_SetReferenceLogBoard"))
            reference_done = 0;
        if (!rival_done || !swofty::api->resolve("site_Game_GhostlapTrace_RivalLogBoard"))
            rival_done = 0;
        std::vector<SwoftyMixin> sites;
        if (reference_done)
            sites.push_back({"site_Game_TimeTrial_SetReferenceLogBoard", SWOFTY_MIXIN_SITE, 100, (void *)&reference_log_board, nullptr});
        if (rival_done)
            sites.push_back({"site_Game_GhostlapTrace_RivalLogBoard", SWOFTY_MIXIN_SITE, 100, (void *)&rival_log_board, nullptr});
        if (sites.empty())
            swofty::log("[leaderboard] time trial reference log guard off: needs site_Game_TimeTrial_SetReferenceLogBoard "
                        "and site_Game_TimeTrial_SetReferenceLogDone, which this build does not resolve");
        else
            out.push_back({REFERENCE_MIXIN, std::move(sites)});
    }
    return out;
}

void leaderboard_guard_commit()
{
    if (!offered || settled)
        return;
    settled = true;
    if (boards_done)
        swofty::log("[leaderboard] time trial leaderboard guard %s",
                    core_mixins_active(MIXIN)
                        ? "armed: View Leaderboards at a track without a board returns instead of faulting"
                        : "did not install (see the [core] lines)");
    if (tab_done)
        swofty::log("[leaderboard] time trial leaderboard tab guard %s",
                    core_mixins_active(TAB_MIXIN)
                        ? "armed: activating the leaderboard tab without a child list model returns instead of faulting"
                        : "did not install (see the [core] lines)");
    if (reference_done || rival_done)
        swofty::log("[leaderboard] time trial reference log guard %s",
                    core_mixins_active(REFERENCE_MIXIN)
                        ? (rival_done && reference_done
                               ? "armed: the game's GAME and GHOSTLAP-TRACE lines for a reference with no board are "
                                 "skipped instead of faulting"
                               : "armed on one of its two log lines (see the [core] lines)")
                        : "did not install (see the [core] lines)");
}

}
