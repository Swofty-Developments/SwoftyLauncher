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

constexpr size_t PLACEMENT_OFFSET = 8;
constexpr size_t SAFETY_CAR_SLOT = 0x738;
constexpr size_t BOX_13_SLOT = 0x838 + 12 * sizeof(uint64_t);

// Game_PitLane_UpdateMovement and Game_PitLane_MoveDownSpline read box 12's placement from the safety car's slot
// (type 7, slot 0, placement + 0x738) and boxes 0 to 11 from type 8 (placement + 0x838 + box * 8). A PitBox13 object
// is type 8, slot 12, so the 13th team's box 12 resolves to the safety car's spot. The redirect puts PitBox13's
// placement into the safety car's slot for the duration of the call and puts the safety car's back after it.
struct Redirect {
    uint64_t *safety_car = nullptr;
    uint64_t saved = 0;

    bool engage(uint64_t *safety_car_slot, const uint64_t *box_13_slot)
    {
        if (!safety_car_slot || !box_13_slot || !*box_13_slot || *box_13_slot == *safety_car_slot)
            return false;
        safety_car = safety_car_slot;
        saved = *safety_car_slot;
        *safety_car_slot = *box_13_slot;
        return true;
    }

    void release()
    {
        if (safety_car)
            *safety_car = saved;
        safety_car = nullptr;
    }
};

// The bad boxes the guard has logged: each of the first LOG_FIRST distinct boxes once.
using Seen = swofty_core::guards::FirstKeys<uint32_t, LOG_FIRST>;

}
