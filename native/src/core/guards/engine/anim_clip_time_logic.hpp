#pragma once

#include <cmath>
#include <limits>

namespace runtime::anim_clip_time {

constexpr int LOG_FIRST = 4;

constexpr float EARLIEST_TIME = std::numeric_limits<float>::lowest();

inline bool time_unordered(float time) { return std::isnan(time); }

inline float sample_time(float time) { return time_unordered(time) ? EARLIEST_TIME : time; }

}
