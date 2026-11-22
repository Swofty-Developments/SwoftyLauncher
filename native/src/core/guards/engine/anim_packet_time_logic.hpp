#pragma once

#include "core/guards/engine/anim_clip_time_logic.hpp"

#include <cstddef>
#include <cstdint>

namespace runtime::anim_packet_time {

constexpr int LOG_FIRST = 4;

// The keyframe packet types whose sampler the guard wraps, in the order of its needs: B1, B4 and L1 sample inline in
// their slot +0x18, L3, L4 and S4 through a per-track sampler that their slots +0x18 and +0x58 call.
constexpr const char *TYPES[] = {"B1", "B4", "L1", "L3", "L4", "S4"};

constexpr size_t TYPE_COUNT = sizeof TYPES / sizeof TYPES[0];

struct Time {
    float time;
    bool replaced;
};

inline Time sampler_time(float time) { return {anim_clip_time::sample_time(time), anim_clip_time::time_not_finite(time)}; }

inline uint64_t offset_in(uintptr_t at, uintptr_t base) { return at >= base ? at - base : at; }

}
