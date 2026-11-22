#pragma once
#include <cctype>
#include <cstdint>
#include <string_view>
#include <utility>

namespace render::postfx {

inline constexpr uint64_t PRESENT_GONE_MS = 1000;

// Whether no Present has reached the core's swapchain hooks for a second since the passes were made (started),
// or since the last one.
inline bool presents_gone(uint64_t now, uint64_t present_at, uint64_t started)
{
    if (!started)
        return false;
    uint64_t last = present_at > started ? present_at : started;
    return now >= last && now - last >= PRESENT_GONE_MS;
}

// Frame generation that presents the game's frames and its own generated ones through the swapchain at its own
// pace: XeSS frame generation's libxess_fg.dll calls Present1 from its own thread, once per frame it shows, so its
// Presents are not the game's frames. `fg` is a Swapchain::fg: a module name, or one with " (calls Present)" after it.
inline bool presents_own_frames(std::string_view fg)
{
    std::string_view name = fg.substr(0, fg.find(' '));
    size_t at = name.find_last_of("\\/");
    name = at == std::string_view::npos ? name : name.substr(at + 1);
    constexpr std::string_view XESS_FG = "libxess_fg.dll";
    if (name.size() != XESS_FG.size())
        return false;
    for (size_t i = 0; i < name.size(); i++)
        if (char(std::tolower(static_cast<unsigned char>(name[i]))) != XESS_FG[i])
            return false;
    return true;
}

// What closes the passes' frame (frame_count, which the placement gate, the depth copy and the previous frame
// read). A swapchain that presents each of the game's frames once closes it at Present. While frame generation
// presents frames of its own (presents_own_frames) or no Present reaches the hooks, the game's own frames close
// it instead, where its lists are submitted: a list that makes a back buffer other than the last one a render
// target starts the next frame, one per frame the game draws whatever frame generation shows. The game's flow
// tick never closes it; while no Present arrives it only keeps the toggle keys and live settings running.
struct FrameClock {
    uint64_t present_at = 0, generated_at = 0;
    const void *drawn = nullptr;

    bool game_frames(uint64_t now, uint64_t started) const
    {
        if (generated_at && now >= generated_at && now - generated_at < PRESENT_GONE_MS)
            return true;
        return presents_gone(now, present_at, started);
    }

    // A Present the hooks dispatch, from frame generation's own presenter or not: true when it closes the frame.
    bool present(uint64_t now, bool generated, uint64_t started)
    {
        present_at = now;
        if (generated)
            generated_at = now;
        return !game_frames(now, started);
    }

    // A submitted list that made back buffer back a render target (nullptr for none): true when it closes the frame.
    bool submit(const void *back, uint64_t now, uint64_t started)
    {
        if (!back || back == drawn)
            return false;
        drawn = back;
        return game_frames(now, started);
    }

    // The game's flow tick: true when it runs the toggle keys and live settings, as no Present arrives.
    bool flow(uint64_t now, uint64_t started) const { return presents_gone(now, present_at, started); }
};

// A frame count far enough back that the passes count as not run lately.
inline constexpr unsigned LONG_AGO = 0u - 8;

// Whether the passes ran on a back buffer at frame at, seen at frame now, within this use of it. A back
// buffer comes round again two or more frames later.
inline bool placed_lately(unsigned now, unsigned at) { return now - at < 2; }

// The HUD's read of the finished picture, which the game makes while paused and under panels over the race: the
// passes run at that read. A frame's lists are recorded in parallel, so the list with the HUD-less step cannot
// tell whether the HUD's list of the same frame reads the picture; while the read holds, the step leaves the passes
// to it. It holds for the frame of a read and the one after. The submission of the picture's list comes after the
// lists recorded before it, so it knows whether the HUD read the picture in this frame yet: when it did, the passes
// ran there. In the frame after a read without one so far (the latch), the HUD's list may still read the picture,
// so the submission leaves the passes waiting for it (BackPlace): a read that comes places them, and without one they
// run at the fallbacks, so the first frame without a read keeps them too. Either way the back buffer is placed once.
struct HudRead {
    unsigned at = LONG_AGO;

    void read(unsigned now) { at = now; }
    bool holds(unsigned now) const { return now - at <= 1; }
    // The HUD read the picture in frame now itself.
    bool read_in(unsigned now) const { return at == now; }
    // A read of the frame before holds the in-list places off and none came in this frame yet.
    bool latch(unsigned now) const { return holds(now) && !read_in(now); }
};

// What the submission of a picture's list does with a fresh back buffer: nothing (the passes were placed), the
// passes between the game's lists right after it, or wait for the list that closes the HUD-less step.
enum PictureEnd : uint8_t { END_NONE, END_BETWEEN, END_DEFER };

// What the submission of a picture's list asks BackPlace::picture_submitted. When the list ends with the HUD-less
// copy still in UAV, the passes wait for the step's closing list while a Present can still fall back (not
// game_frames), and otherwise run right after the list. In the HUD read's latch (HudRead::latch) they always wait:
// running them right after the list would take the place of a read the HUD's list still makes in this frame, and
// with it the hold that keeps the read the one place on track. Without a Present to fall back on, a latch frame
// whose HUD's list the core does not see goes without them, as every latch frame did before.
struct PictureWait {
    bool open = false, may_defer = false;
};
inline PictureWait picture_wait(bool step_open, bool latch, bool game_frames)
{
    return {step_open || latch, latch || (step_open && !game_frames)};
}

// Where one back buffer's passes go in a frame. It is fresh from the barrier that makes it a render target until
// the passes are placed on it. A picture's list can end inside the HUD-less step; the list that closes the step
// then sometimes draws the rest of the picture into the back buffer, over anything placed after the picture's list.
// So that submission only opens the step: the closing list runs the passes in it while the back buffer is bound
// there (the split step); when it doesn't, the core's own list runs them right after the closing list. A list that
// ends with the back buffer out of render target while they still wait is the one that draws the HUD and hands the
// frame to Present: they run right before it. Only a frame whose lists the core never saw do that reaches Present
// with them waiting, and they run there, over the HUD: the picture keeps its look and the HUD alone differs for
// that frame, where skipping them would change the whole picture. `target` is whether the lists submitted since
// the step opened left the back buffer a render target.
struct BackPlace {
    bool fresh = false, open = false, target = false;
    unsigned placed_at = LONG_AGO, opened_at = LONG_AGO;

    // The barrier that makes the back buffer a render target for a new frame.
    void drawn(unsigned now)
    {
        fresh = !placed_lately(now, placed_at);
        open = false;
        target = true;
    }

    // Whether a list that closes the HUD-less step with this back buffer bound runs the passes in it.
    bool split_step(unsigned now) const { return fresh || !placed_lately(now, placed_at); }

    void place(unsigned now)
    {
        fresh = false;
        placed_at = now;
    }

    // The list the passes were recorded into was reset unsubmitted.
    void unplace()
    {
        fresh = true;
        placed_at = LONG_AGO;
    }

    // The picture's list is submitted with the back buffer still a render target; step_open when it ends with the
    // HUD-less copy still in UAV. The step opens even when a closing list recorded first already holds the passes,
    // so a reset of that list leaves them to the fallbacks.
    PictureEnd picture_submitted(unsigned now, bool step_open, bool may_defer)
    {
        if (step_open && may_defer) {
            open = true;
            opened_at = now;
            target = true;
            return fresh ? END_DEFER : END_NONE;
        }
        if (!fresh)
            return END_NONE;
        place(now);
        return END_BETWEEN;
    }

    // A list submitted after the picture's moved the back buffer.
    void moved(bool still_target)
    {
        if (open)
            target = still_target;
    }

    // Whether the passes still wait for the step that the picture's list left open.
    bool waits(unsigned now) const { return open && fresh && now - opened_at < 2; }

    // A list that closed the step is submitted: true when the passes run between, right after it.
    bool step_closed(unsigned now, bool direct)
    {
        if (!direct || !target || !waits(now))
            return false;
        place(now);
        return true;
    }

    // A list submitted after the picture's ends with the back buffer out of render target: true when the passes
    // run between, right before it. Asked before moved().
    bool target_left(unsigned now, bool direct)
    {
        if (!direct || !target || !waits(now))
            return false;
        place(now);
        return true;
    }

    // The back buffer is presented: true when the passes run at Present.
    bool presented(unsigned now)
    {
        if (!waits(now))
            return false;
        place(now);
        return true;
    }
};

// A frame generation swapchain (FidelityFX's, Streamline's) hands the game back buffers of its own and copies each
// frame into its real back buffer inside Present, so the buffer a Present shows is never one the game drew into and
// never matches the back buffer whose passes wait. The shown buffer still holds that picture when frame generation
// is off (each Present shows the frame the game just drew), the step opened in the frame this Present closes, no
// passes were recorded since the last Present, and the two buffers have the same size and format.
inline bool shown_is_waiting(bool generating, unsigned runs, unsigned now, unsigned opened_at, bool same_shape)
{
    return !generating && !runs && now == opened_at && same_shape;
}

// Why a presented frame went without the passes, most specific first.
enum Idle : uint8_t {
    IDLE_NONE,
    IDLE_KILL,
    IDLE_KEY,
    IDLE_PASS_OFF,
    IDLE_RULE,
    IDLE_OPTION,
    IDLE_ADDON_KEY,
    IDLE_VIEW,
    IDLE_PASSES,
    IDLE_BUSY,
    IDLE_LIST_STATE,
    IDLE_UI_TOGETHER,
    IDLE_HUD_READ,
    IDLE_NO_PICTURE,
    IDLE_UNPLACED,
    IDLE_COUNT
};

// What a frame knows when it closes without the passes: the kill file, the shader toggle key, what stopped the
// place that tried to run them (IDLE_NONE when none tried), whether the HUD's read held the other places off, and
// whether the game submitted a picture list.
struct IdleFacts {
    bool killed = false, key_off = false, hud_read = false, picture = false;
    Idle tried = IDLE_NONE;
};

inline Idle idle_reason(const IdleFacts &f)
{
    if (f.killed)
        return IDLE_KILL;
    if (f.key_off)
        return IDLE_KEY;
    if (f.tried != IDLE_NONE)
        return f.tried;
    if (f.hud_read)
        return IDLE_HUD_READ;
    return f.picture ? IDLE_UNPLACED : IDLE_NO_PICTURE;
}

// Why no pass of a frame is live, from the reasons its passes gave (one bit per Idle): the reason they share, or
// IDLE_PASSES when they differ.
inline Idle idle_of_passes(unsigned reasons)
{
    for (Idle one : {IDLE_PASS_OFF, IDLE_RULE, IDLE_OPTION, IDLE_ADDON_KEY, IDLE_VIEW})
        if (reasons == 1u << one)
            return one;
    return IDLE_PASSES;
}

inline const char *idle_text(Idle why)
{
    switch (why) {
    case IDLE_KILL:
        return "swofty/postfx.off exists";
    case IDLE_KEY:
        return "the shader toggle key turned them off";
    case IDLE_PASS_OFF:
        return "every pass is off, over its GPU budget or without a pipeline";
    case IDLE_RULE:
        return "a rule turned every pass off";
    case IDLE_OPTION:
        return "an option turns every pass off";
    case IDLE_ADDON_KEY:
        return "an addon's shader key turned every pass off";
    case IDLE_VIEW:
        return "no pass is for this view";
    case IDLE_PASSES:
        return "every pass is off or not for this view";
    case IDLE_BUSY:
        return "every slot is in flight";
    case IDLE_LIST_STATE:
        return "the game's list has state from before the hooks";
    case IDLE_UI_TOGETHER:
        return "the picture and the HUD are in one list without a HUD-less step";
    case IDLE_HUD_READ:
        return "the HUD read the picture a frame before and not in this one (pause latch)";
    case IDLE_NO_PICTURE:
        return "the game submitted no picture list";
    case IDLE_UNPLACED:
        return "no list of the frame had a place for them";
    default:
        return "";
    }
}

// The passes going off and on again, for one log line per change. The first time they run has its own line.
struct OnOff {
    enum Change : uint8_t { SAME, STOPPED, AGAIN };
    bool on = false, ran_before = false;
    unsigned off_at = 0;
    Idle why = IDLE_NONE;

    Change frame(bool ran, unsigned now, Idle reason)
    {
        if (ran == on)
            return SAME;
        on = ran;
        if (!ran) {
            off_at = now;
            why = reason;
            return STOPPED;
        }
        return std::exchange(ran_before, true) ? AGAIN : SAME;
    }
};

// The count line's window: a minute for the first half hour, five minutes for the rest of the session.
inline constexpr unsigned STAT_MINUTES = 30, ONOFF_LINES = 20;
inline unsigned stat_window_ms(unsigned lines) { return lines < STAT_MINUTES ? 60000 : 300000; }

}
