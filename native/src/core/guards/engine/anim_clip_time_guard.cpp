#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/engine/anim_clip_time_logic.hpp"

#include <swofty/mixin.hpp>

// Anim clip time guard (re/bindings/core/Ne_AnimClipPlayer_Sample_Thunk.json).
//
// An animation clip player samples its clip at a time through Ne_AnimClipPlayer_Sample_Thunk: the body calls slot
// +0x18 of every packet in the clip, and the keyframe packets hand the time to a per-track sampler. The sampler
// compares the time only with the track's first key (vucomiss; jae), then turns (time - first key) * rate into a
// bucket index with vcvttss2si. A time that is not a number fails the compare, the index comes out as 0x80000000, the
// signed bucket-count test passes it, and the sampler reads the bucket table 4 GB below its start: ACCESS_VIOLATION at
// F1_25.exe+0x3a3d54b (Steam 26566308) or +0x3a2a56b (EA 1.0.145). Every keyframe packet type has the same path. The
// player that faults is the cockpit steering_steer clip (car visual +0x1370), sampled from Car_Visual_UpdateRenderState
// and from the in-car driver update. Its time follows the steering wheel's spring state, and the spring update's clamps
// pass a value that is not a number through, so once the state is not a number it stays so. Seen at a session start,
// after a flashback, after the podium cutscene and on resume.
//
// The guard wraps the thunk and passes the lowest float in place of a time that is not a number. Each sampler then
// takes its own before-the-first-key exit and samples the clip's first key. Every other time runs the game's code
// unchanged.
//
// SWOFTY_ANIM_CLIP_TIME_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (anim clip time guard)";
constexpr const char *SAMPLE = "Ne_AnimClipPlayer_Sample_Thunk";

namespace guards = swofty_core::guards;
namespace act = runtime::anim_clip_time;

using Sample = void(void *, float);
Sample *orig_sample = nullptr;
guards::LogFirst<act::LOG_FIRST> replaced{"replacements"};

void sample(void *player, float time)
{
    if (act::time_unordered(time)) {
        if (auto line = replaced.note())
            swofty::log("[anim] an animation clip got a time that is not a number, so it shows the clip's first frame%s",
                        line.tail);
    }
    orig_sample(player, act::sample_time(time));
}

guards::SiteGuard guard{guards::CoreSetId::anim_clip_time,
                        {
                            .set = MIXIN,
                            .name = "[anim] clip time guard",
                            .env = "SWOFTY_ANIM_CLIP_TIME_GUARD",
                            .needs = {SAMPLE},
                            .mixins = {{SAMPLE, SWOFTY_MIXIN_WRAP, 100, (void *)&sample, (void **)&orig_sample}},
                            .armed = "an animation clip sampled at a time that is not a number shows its first frame "
                                     "instead of faulting",
                        }};

}
