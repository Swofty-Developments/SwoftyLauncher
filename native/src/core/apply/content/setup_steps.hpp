#pragma once

#include <array>
#include <cstdint>

// The 42 slider steps a Car_Setup_ApplyTuning call applies (0x140c11550, car.md#tuning). Its third argument is a setup
// block: the ints are at block+8, or at block+0xb0 when the byte block+0x158 is set (the parc ferme copy), one per
// car_setup.sliders index in the order of setup_locks.hpp. The steps are what the setup screen stores (the VM reads
// them at entry+0x70+4*i+slot*0x550, 0x140c1b9d0) and what a saved setup keeps (0xa8 bytes of ints). Pure, for the
// host test.

namespace swofty_core::apply::content {

inline constexpr int SETUP_SLIDER_COUNT = 42;
inline constexpr uintptr_t SETUP_BLOCK_STEPS = 0x08;
inline constexpr uintptr_t SETUP_BLOCK_STEPS_ALT = 0xb0;
inline constexpr uintptr_t SETUP_BLOCK_SELECT = 0x158;

using SetupSteps = std::array<int32_t, SETUP_SLIDER_COUNT>;

template <class Read> bool read_setup_steps(uintptr_t block, Read &&read, SetupSteps &out)
{
    if (!block)
        return false;
    uint8_t select = 0;
    if (!read(block + SETUP_BLOCK_SELECT, &select, sizeof select))
        return false;
    uintptr_t at = block + (select ? SETUP_BLOCK_STEPS_ALT : SETUP_BLOCK_STEPS);
    SetupSteps tmp{};
    if (!read(at, tmp.data(), sizeof tmp))
        return false;
    out = tmp;
    return true;
}

}
