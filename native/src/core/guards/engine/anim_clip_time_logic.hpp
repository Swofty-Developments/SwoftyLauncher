#pragma once

#include <cmath>
#include <limits>

namespace runtime::anim_clip_time {

constexpr int LOG_FIRST = 4;

constexpr float EARLIEST_TIME = std::numeric_limits<float>::lowest();

inline bool time_not_finite(float time) { return !std::isfinite(time); }

inline float sample_time(float time) { return time_not_finite(time) ? EARLIEST_TIME : time; }

}
