#pragma once

#include <cmath>
#include <limits>

namespace runtime::tyre_curve {

constexpr int LOG_FIRST = 4;

constexpr float LOWEST_KEY = std::numeric_limits<float>::lowest();

inline bool key_unordered(float key) { return std::isnan(key); }

inline float lookup_key(float key) { return key_unordered(key) ? LOWEST_KEY : key; }

}
