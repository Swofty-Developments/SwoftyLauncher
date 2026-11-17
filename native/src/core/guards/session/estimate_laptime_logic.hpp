#pragma once

#include "core/guards/skips.hpp"

#include <cstdint>

namespace runtime::estimate_laptime {

constexpr int LOG_FIRST = 4;

inline bool tyre_ctx_missing(uint64_t ctx)
{
    return ctx == 0;
}

using Skips = swofty_core::guards::Skips<LOG_FIRST>;

}
