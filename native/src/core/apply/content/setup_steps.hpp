#pragma once

#include <array>
#include <cstdint>
#include <cstring>

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

inline constexpr uintptr_t SETUP_ENTRY_BLOCKS = 0x68, SETUP_ENTRY_SLOT_SIZE = 0x550;

// The block Car_Setup_SetSliderValue (0x140c11160) hands to ApplyTuning, from the setup entry alone: the entry's active
// slot (entry+1) at entry+0x68+slot*0x550, or, while entry+0x30 is set and entry+0x60 is not, the fixed block its mode
// (entry+0x34) names (0x15a8, 0x1058, 0xb08, 0x5b8, 0x68; any other mode the slot at entry+0). The slot is not limited
// to the four race slots: the setup screen edits others. 0 when the entry cannot be read.
template <class Read> uintptr_t setup_entry_block(uintptr_t entry, Read &&read)
{
    if (!entry)
        return 0;
    uint8_t head[0x68]{};
    if (!read(entry, head, sizeof head))
        return 0;
    uint32_t mode = 0;
    std::memcpy(&mode, head + 0x34, sizeof mode);
    if (head[0x30] != 0 && head[0x60] == 0) {
        switch (mode) {
        case 0:
            return entry + 0x15a8;
        case 1:
            return entry + 0x1058;
        case 2:
            return entry + 0xb08;
        case 3:
            return entry + 0x5b8;
        case 4:
            return entry + SETUP_ENTRY_BLOCKS;
        default:
            return entry + SETUP_ENTRY_BLOCKS + uintptr_t(head[0]) * SETUP_ENTRY_SLOT_SIZE;
        }
    }
    return entry + SETUP_ENTRY_BLOCKS + uintptr_t(head[1]) * SETUP_ENTRY_SLOT_SIZE;
}

}
