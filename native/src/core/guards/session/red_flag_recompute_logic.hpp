#pragma once

#include <cstdint>

namespace runtime::red_flag_recompute {

constexpr int LOG_FIRST = 4;

inline bool record_missing(uint64_t race_session_data) { return race_session_data == 0; }

}
