#pragma once

#include <cstdint>

// Whether a queued new personal best is written back into telemetry lap 3 (the stored personal best, the lap the in-game delta
// runs against). Pure: the game thread feeds it what it read and acts on the answer.
//
// The game's own handler resets lap 3 a few milliseconds after a new personal best (Game_GhostLapHandler_Reset on the commit
// path) and reloads it from the board's download. On our boards the download is ours and the lap is armed again in sync. On the
// game's own boards the reload happens only when the game is online; offline the entry is unusable, lap 3 stays empty and every
// later valid lap is a "new personal best" with no delta. So the lap is armed again whenever lap 3 is empty after the reset, on
// any content; a lap 3 that still holds a trace on the game's own board is the game's and is left alone.
namespace runtime::ghost_rearm {

constexpr int64_t SETTLE_MS = 600, LATE_MS = 30000;

enum class Do { wait, arm, off, no_laps, not_time_trial, late, game_pb, unbound };

struct Facts {
    bool enabled = false, laps = false, time_trial = false, ours = false, lap3_bound = false;
    uint32_t lap3_count = 0;
    int64_t waited_ms = 0;
};

inline Do decide(const Facts &f)
{
    if (!f.enabled)
        return Do::off;
    if (!f.laps)
        return Do::no_laps;
    if (!f.time_trial)
        return Do::not_time_trial;
    if (f.lap3_count > 0 && f.waited_ms < SETTLE_MS)
        return Do::wait;
    if (f.waited_ms > LATE_MS)
        return Do::late;
    if (!f.ours && f.lap3_count > 0)
        return Do::game_pb;
    if (!f.lap3_bound)
        return Do::unbound;
    return Do::arm;
}

inline const char *why(Do d)
{
    switch (d) {
    case Do::off:
        return "the re-arm is off (SWOFTY_TT_PB_REARM=0) or the trace reader and reset were not found";
    case Do::no_laps:
        return "telemetry lap 1 or lap 3 is missing";
    case Do::not_time_trial:
        return "the session is not a time trial";
    case Do::late:
        return "too late to start in sync";
    case Do::game_pb:
        return "the game's own board kept or reloaded its personal best in lap 3";
    case Do::unbound:
        return "lap 3 is not bound to a car";
    default:
        return "";
    }
}

// At most LOG_PER_CAUSE lines per cause for the life of the process, so a bundle shows why without a line per lap.
constexpr int LOG_PER_CAUSE = 6;

inline bool log_due(int &seen) { return ++seen <= LOG_PER_CAUSE; }

}
