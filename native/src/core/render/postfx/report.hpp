#pragma once
#include "core/frontend/view_state.hpp"
#include "core/render/postfx/frame_logic.hpp"
#include "core/render/postfx/view_logic.hpp"

#include <windows.h>

#include <d3d12.h>

#include <string>

namespace render::postfx {

// What the passes did, frame by frame: the counts behind the periodic stat line, the line when they stop and
// when they run again, and the per-frame trace (SWOFTY_POSTFX_TRACE, swofty/postfx.trace).
class FrameReport {
public:
    struct Stats {
        unsigned presented = 0, in_list = 0, split_step = 0, between = 0, after_close = 0, before_hud = 0, at_present = 0,
                 without = 0, doubled = 0, ui_together = 0, unknown_state = 0, busy = 0, deferred = 0, dropped = 0;
        unsigned generated = 0, runs = 0, unplaced = 0;
        unsigned copied = 0, hud_read = 0, no_previous = 0, prev_gap = 0, onoff_untold = 0, idle[IDLE_COUNT] = {};
    };

    // One presented frame.
    struct Trace {
        const char *path = "none", *why = nullptr;
        Idle tried = IDLE_NONE;
        bool picture = false;
        unsigned runs = 0, ran = 0, live = 0;
        std::string skipped;
        ID3D12Resource *back = nullptr;
        frontend::view::State view;
    };

    Stats stats;
    Trace trace;

    bool tracing() const { return trace_left != 0; }
    void trace_next(unsigned frames) { trace_left = frames; }
    // Why the frame that closes went without the passes, counted, and one line when they stop and when they run
    // again (ONOFF_LINES a count line at most; the count line says how many were left out).
    void note_idle(bool killed, bool shaders_off, bool hud_reads, unsigned frame_count, const std::string &toggle_key);
    void log_trace(unsigned frame_count, bool fg_on);
    void stat_line(ULONGLONG now, bool game_frames, bool have_passes, unsigned ran_total);

private:
    unsigned stat_lines = 0, onoff_lines = 0;
    ULONGLONG stat_at = 0, stat_from = 0;
    OnOff onoff;
    unsigned trace_left = 0, trace_present = 0;
};

}
