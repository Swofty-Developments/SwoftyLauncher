#pragma once

#include <cstdint>

namespace runtime::pit_garage_state {

// The pit lane system's per-garage state words (PitLaneSystem +0x210, u32 0, 1 or 2) are indexed by the slot the game
// takes from a car's pit lane box (+0xe0): box >> 1, 12 for the box 99. The game's own slots end at 13, the one
// Game_PitLane_PutVehicleInPitBox gives a car past index 26.
constexpr uint32_t SLOTS = 14;
constexpr uint32_t SPECIAL_BOX = 99, SPECIAL_SLOT = 12;

constexpr uint32_t slot_of(uint32_t box) { return box == SPECIAL_BOX ? SPECIAL_SLOT : box >> 1; }

constexpr bool slot_usable(uint64_t slot) { return slot < SLOTS; }

}
