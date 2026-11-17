#pragma once

#include "core/guards/skips.hpp"

#include <cstdint>

namespace runtime::thrustmaster_lcd {

constexpr int LOG_FIRST = 4;

inline bool record_missing(uint64_t record)
{
    return record == 0;
}

using Skips = swofty_core::guards::Skips<LOG_FIRST>;

}
