#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/engine/anim_packet_time_logic.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>

// Anim packet time guard (re/bindings/core/Ne_AnimPacket*_Sample*.json).
//
// A keyframe animation packet samples its tracks at a time through slot +0x18 of its vtable. The keyframe samplers
// compare the time only with a track's first key (vucomiss; jae), then turn (time - first key) * rate into a bucket
// index with vcvttss2si. Neither a time that is not a number nor plus infinity takes the first-key branch, the convert
// turns both into 0x80000000, the signed bucket-count test passes it, and the sampler reads the bucket table 4 GB below
// its start. The anim clip time guard covers the clip player thunk, but other callers reach slot +0x18 with their own
// time: the UI animation player (Steam 26566308 0x1441fb320, KB 0x1441f8d50) samples its packets at its own time and
// faulted in the B1 packet at F1_25.exe+0x3a3bfaa (Steam 26566308) with a time that is not a number, shortly after
// Terminal Damage tore a wheel off the player's car.
//
// The guard wraps the samplers of the six packet types that have that path, so every caller is covered: the slot +0x18
// functions of B1, B4 and L1, which sample inline, and the per-track samplers of L3, L4 and S4, which their slots +0x18
// and +0x58 call. The fp16 types sample through a helper that clamps a negative index to the first frame, and the ntk
// types do not read the time. A time that is not a finite number (not a number, plus or minus infinity) becomes the
// lowest float, so the sampler takes its own before-the-first-key exit, which minus infinity already took: 3 or more
// keys sample segment 0 at weight 0, 2 keys clamp the weight to 0, 1 key ignores the time. Every finite time runs the
// game's code unchanged, so a finite time far enough past a track's first key that (time - first key) * rate reaches
// 2^31 still faults.
//
// SWOFTY_ANIM_PACKET_TIME_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (anim packet time guard)";
constexpr const char *SAMPLE_B1 = "Ne_AnimPacketB1_Sample";
constexpr const char *SAMPLE_B4 = "Ne_AnimPacketB4_Sample";
constexpr const char *SAMPLE_L1 = "Ne_AnimPacketL1_Sample";
constexpr const char *SAMPLE_L3 = "Ne_AnimPacketL3_SampleTrack";
constexpr const char *SAMPLE_L4 = "Ne_AnimPacketL4_SampleTrack";
constexpr const char *SAMPLE_S4 = "Ne_AnimPacketS4_SampleTrack";

namespace guards = swofty_core::guards;
namespace apt = runtime::anim_packet_time;

using Sample = void(void *, void *, float, void *);
using SampleTrack = void(void *, void *, uint32_t, float, void *);

Sample *orig_b1 = nullptr;
Sample *orig_b4 = nullptr;
Sample *orig_l1 = nullptr;
SampleTrack *orig_l3 = nullptr;
SampleTrack *orig_l4 = nullptr;
SampleTrack *orig_s4 = nullptr;
guards::LogFirst<apt::LOG_FIRST> replaced{"replacements"};

float sampler_time(float time, size_t type, void *caller)
{
    apt::Time t = apt::sampler_time(time);
    if (t.replaced) {
        if (auto line = replaced.note())
            swofty::log("[anim] an animation packet (%s) got a time that is not a finite number from F1_25.exe+0x%llx, so "
                        "it shows its first key%s",
                        apt::TYPES[type],
                        (unsigned long long)apt::offset_in(uintptr_t(caller), uintptr_t(GetModuleHandleW(nullptr))), line.tail);
    }
    return t.time;
}

void sample_b1(void *packet, void *binding, float time, void *out)
{
    orig_b1(packet, binding, sampler_time(time, 0, __builtin_return_address(0)), out);
}

void sample_b4(void *packet, void *binding, float time, void *out)
{
    orig_b4(packet, binding, sampler_time(time, 1, __builtin_return_address(0)), out);
}

void sample_l1(void *packet, void *binding, float time, void *out)
{
    orig_l1(packet, binding, sampler_time(time, 2, __builtin_return_address(0)), out);
}

void sample_l3(void *packet, void *track, uint32_t index, float time, void *out)
{
    orig_l3(packet, track, index, sampler_time(time, 3, __builtin_return_address(0)), out);
}

void sample_l4(void *packet, void *track, uint32_t index, float time, void *out)
{
    orig_l4(packet, track, index, sampler_time(time, 4, __builtin_return_address(0)), out);
}

void sample_s4(void *packet, void *track, uint32_t index, float time, void *out)
{
    orig_s4(packet, track, index, sampler_time(time, 5, __builtin_return_address(0)), out);
}

guards::SiteGuard guard{guards::CoreSetId::anim_packet_time,
                        {
                            .set = MIXIN,
                            .name = "[anim] packet time guard",
                            .env = "SWOFTY_ANIM_PACKET_TIME_GUARD",
                            .needs = {SAMPLE_B1, SAMPLE_B4, SAMPLE_L1, SAMPLE_L3, SAMPLE_L4, SAMPLE_S4},
                            .mixins = {{SAMPLE_B1, SWOFTY_MIXIN_WRAP, 100, (void *)&sample_b1, (void **)&orig_b1},
                                       {SAMPLE_B4, SWOFTY_MIXIN_WRAP, 100, (void *)&sample_b4, (void **)&orig_b4},
                                       {SAMPLE_L1, SWOFTY_MIXIN_WRAP, 100, (void *)&sample_l1, (void **)&orig_l1},
                                       {SAMPLE_L3, SWOFTY_MIXIN_WRAP, 100, (void *)&sample_l3, (void **)&orig_l3},
                                       {SAMPLE_L4, SWOFTY_MIXIN_WRAP, 100, (void *)&sample_l4, (void **)&orig_l4},
                                       {SAMPLE_S4, SWOFTY_MIXIN_WRAP, 100, (void *)&sample_s4, (void **)&orig_s4}},
                            .armed = "an animation packet sampled at a time that is not a finite number shows its first "
                                     "key instead of faulting",
                        }};

}
