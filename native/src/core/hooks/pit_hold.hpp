#pragma once

#include <cstdint>

#include "swofty_api.h"

// The extra box hold of the session operations (pit_hold.cpp, pit_hold_logic.hpp).
namespace swofty_core::hooks::pit_hold {

// Arms car's next stop with seconds of extra hold (0 disarms). SWOFTY_ASSET_UNAVAILABLE when the hold did not install
// on this build, SWOFTY_ASSET_INVALID for a bad car.
int arm(uint32_t car, float seconds);
// The car's hold: what the next stop holds, the stop in progress and the last served stop.
int read(uint32_t car, SwoftyPitHold *out);
// A pit status change, from the core's Game_Pitlane_SetVehicleStatus detour.
void on_status(uint32_t car, uint32_t before, uint32_t after);
// A new session's rule manager: every car's hold starts over.
void reset();

}
