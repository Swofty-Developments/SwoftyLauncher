#include "core/render/postfx/report.hpp"

#include <swofty/mixin.hpp>

namespace render::postfx {

void FrameReport::note_idle(bool killed, bool shaders_off, bool hud_reads, unsigned frame_count, const std::string &toggle_key)
{
    Idle why = IDLE_NONE;
    if (!trace.runs) {
        why = idle_reason({killed, shaders_off, hud_reads, trace.picture, trace.tried});
        stats.idle[why]++;
    }
    OnOff::Change c = onoff.frame(trace.runs > 0, frame_count, why);
    if (c == OnOff::SAME)
        return;
    if (onoff_lines >= ONOFF_LINES) {
        stats.onoff_untold++;
        return;
    }
    onoff_lines++;
    frontend::view::State v = frontend::view::current();
    std::string view = std::string(view_text(v.kind)), flags = flag_text(v.flags);
    if (c == OnOff::STOPPED)
        swofty::log("[postfx] the passes stopped at frame %u: %s%s%s (view %s, flags %s, serial %u)", frame_count, idle_text(why),
                    why == IDLE_KEY ? ", key " : "", why == IDLE_KEY ? toggle_key.c_str() : "", view.c_str(), flags.c_str(),
                    v.serial);
    else
        swofty::log("[postfx] the passes run again at frame %u, %s, after %u frame(s) without them: %s (view %s, "
                    "flags %s, serial %u)",
                    frame_count, trace.path, frame_count - onoff.off_at, idle_text(onoff.why), view.c_str(), flags.c_str(),
                    v.serial);
}

void FrameReport::log_trace(unsigned frame_count, bool fg_on)
{
    if (!trace_left)
        return;
    trace_left--;
    swofty::log("[postfx] trace present=%u f=%u path=%s runs=%u passes=%u/%u view=%s flags=%s valid=%d serial=%u fg=%s "
                "back=%p%s%s%s%s",
                trace_present++, frame_count, trace.path, trace.runs, trace.ran, trace.live,
                std::string(view_text(trace.view.kind)).c_str(), flag_text(trace.view.flags).c_str(), int(trace.view.valid),
                trace.view.serial, fg_on ? "on" : "off", static_cast<void *>(trace.back), trace.why ? " why=" : "",
                trace.why ? trace.why : "", trace.skipped.empty() ? "" : " skipped=", trace.skipped.c_str());
}

void FrameReport::stat_line(ULONGLONG now, bool game_frames, bool have_passes, unsigned ran_total)
{
    if (now < stat_at)
        return;
    bool told = false;
    unsigned secs = unsigned((now - stat_from + 500) / 1000);
    const unsigned *idle = stats.idle;
    if (stat_at && game_frames && have_passes) {
        told = true;
        swofty::log("[postfx] %s: %u real frame(s), %u pass run(s), %u skipped in the last %u s (%u in the game's list, "
                    "%u between its lists, %u with the GPU behind); %u Present(s) from frame generation; %u run(s) "
                    "without the frame before's result (%u after a frame gap); %u on/off line(s) left out",
                    stats.generated ? "xess fg" : "no present", stats.presented, stats.runs, stats.unplaced, secs, stats.in_list,
                    stats.between, stats.busy, stats.generated, stats.no_previous, stats.prev_gap, stats.onoff_untold);
    } else if (stat_at && ran_total) {
        told = true;
        swofty::log("[postfx] last %u s: %u frame(s) presented; passes ran in the game's list on %u (%u at the HUD's "
                    "read of the picture, %u in a split step's closing list), between its lists on %u (%u after a "
                    "split step's closing list, %u before the list that takes the back buffer to the HUD) and at "
                    "Present, over the HUD, on %u (%u on a frame generation swapchain's copy of the frame); %u picture "
                    "list(s) left the HUD-less step to a later list; %u "
                    "recorded into lists the game reset unsubmitted; %u without passes (%u toggle key, %u "
                    "swofty/postfx.off, %u after the HUD's last read of the picture, %u every pass off: %u budget or "
                    "pipeline, %u rule, %u option, %u addon key, %u view, %u mixed; %u with the picture and the HUD "
                    "in one list, %u with list state from before the hooks, %u with the GPU behind, %u with no "
                    "picture list, %u with no place in the frame's lists); %u frame(s) ran them twice; %u run(s) "
                    "without the frame before's result (%u after a frame gap); %u on/off line(s) left out",
                    secs, stats.presented, stats.in_list, stats.hud_read, stats.split_step, stats.between, stats.after_close,
                    stats.before_hud, stats.at_present, stats.copied, stats.deferred, stats.dropped, stats.without,
                    idle[IDLE_KEY], idle[IDLE_KILL], idle[IDLE_HUD_READ],
                    idle[IDLE_PASS_OFF] + idle[IDLE_RULE] + idle[IDLE_OPTION] + idle[IDLE_ADDON_KEY] + idle[IDLE_VIEW] +
                        idle[IDLE_PASSES],
                    idle[IDLE_PASS_OFF], idle[IDLE_RULE], idle[IDLE_OPTION], idle[IDLE_ADDON_KEY], idle[IDLE_VIEW],
                    idle[IDLE_PASSES], idle[IDLE_UI_TOGETHER], idle[IDLE_LIST_STATE], idle[IDLE_BUSY], idle[IDLE_NO_PICTURE],
                    idle[IDLE_UNPLACED], stats.doubled, stats.no_previous, stats.prev_gap, stats.onoff_untold);
    }
    stat_lines += told;
    stat_from = now;
    stat_at = now + stat_window_ms(stat_lines);
    stats = Stats{};
    onoff_lines = 0;
}

}
