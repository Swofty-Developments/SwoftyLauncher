#pragma once

#include "core/guards/skips.hpp"

#include <cstddef>
#include <cstdint>

namespace runtime::pit_box {

constexpr uint32_t BOX_SLOTS = 24;
constexpr uint32_t REGISTRY_SLOTS = 26;
constexpr size_t LOG_FIRST = 8;

inline bool box_usable(uint32_t box) { return box < BOX_SLOTS; }

inline bool registry_full(uint32_t count) { return count >= REGISTRY_SLOTS; }

// The bad boxes the guard has logged: each of the first LOG_FIRST distinct boxes once.
using Seen = swofty_core::guards::FirstKeys<uint32_t, LOG_FIRST>;

}
