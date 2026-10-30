// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstddef>
#include <cstdint>

#include "weekend.hpp"

namespace f1990 {

// The seven slots of an EngineManagementComponent record (+0x18, 0x10 apart), in the game's order.
enum PuSlot : uint32_t {
    kSlotIce = 0,
    kSlotMguK = 1,
    kSlotMguH = 2,
    kSlotEs = 3,
    kSlotTc = 4,
    kSlotCe = 5,
    kSlotGearbox = 6,
    kSlotCount = 7,
};

// 1990 parts on the stock slots: the race engine on ICE, the qualifying engine on MGU-K, the clutch on
// ES, the fuel system on TC, the electronics on CE, the event gearbox on its own slot. MGU-H has no
// component config in the 2026 set the 1990 career runs on, and stays hidden.
constexpr uint32_t kRaceEngineSlot = kSlotIce;
constexpr uint32_t kQualiEngineSlot = kSlotMguK;
constexpr uint32_t kClutchSlot = kSlotEs;
constexpr uint32_t kFuelSystemSlot = kSlotTc;
constexpr uint32_t kElectronicsSlot = kSlotCe;

constexpr size_t kPuRecordSize = 0xa60;
constexpr size_t kPuSlotsAt = 0x18;
constexpr size_t kHolderWearMin = 0x3c;
constexpr size_t kHolderWearMax = 0x40;

// The component sets of an EngineManagementComponent (*(mgmt +0x340) +0x50..+0x58, 0x250 bytes
// each: season byte +0, then one 0x54-byte config per slot from +4, allocation u32 at +0x24).
constexpr size_t kSetSize = 0x250;
constexpr size_t kConfigsAt = 0x04;
constexpr size_t kConfigSize = 0x54;
constexpr size_t kAllocationAt = 0x24;

inline bool slot_listed(uint32_t slot)
{
    return slot < kSlotCount && slot != kSlotMguH;
}

// Which slots take session wear in a session of this group (-1: not known). The qualifying
// engine runs in the qualifying groups only, the race engine everywhere else; the clutch, the
// fuel system, the electronics and the gearbox run in every session; MGU-H never wears.
inline bool slot_wears(uint32_t slot, int group)
{
    bool quali = group == 1 || group == 2;
    switch (slot) {
    case kRaceEngineSlot:
        return !quali;
    case kQualiEngineSlot:
        return quali;
    case kClutchSlot:
    case kFuelSystemSlot:
    case kElectronicsSlot:
    case kSlotGearbox:
        return true;
    default:
        return false;
    }
}

// The 1990 allocation of a slot, 0 where the component set's own value stays.
inline uint32_t slot_allocation(uint32_t slot)
{
    switch (slot) {
    case kClutchSlot:
        return 6;
    case kFuelSystemSlot:
        return 5;
    case kElectronicsSlot:
        return 5;
    default:
        return 0;
    }
}

// The wear range of a 1990 part as a multiple of the race engine's range (the same driver's ICE
// holder, R&D and difficulty factors included), 0 where the slot keeps its own range.
inline float slot_wear_scale(uint32_t slot)
{
    switch (slot) {
    case kClutchSlot:
        return 1.2f;
    case kFuelSystemSlot:
        return 0.9f;
    case kElectronicsSlot:
        return 0.8f;
    default:
        return 0.f;
    }
}

// Writes the 1990 allocations into every set of [begin, end); returns the number of values changed.
inline size_t apply_allocations(uint8_t *begin, uint8_t *end)
{
    if (!begin || end <= begin || size_t(end - begin) % kSetSize || size_t(end - begin) / kSetSize > 16)
        return 0;
    size_t changed = 0;
    for (uint8_t *set = begin; set < end; set += kSetSize)
        for (uint32_t slot = 0; slot < kSlotCount; slot++) {
            uint32_t want = slot_allocation(slot);
            uint8_t *field = set + kConfigsAt + slot * kConfigSize + kAllocationAt;
            if (want && at<uint32_t>(field, 0) != want) {
                std::memcpy(field, &want, 4);
                changed++;
            }
        }
    return changed;
}

// The record of a driver in the EngineManagementComponent's record vector (+0x2a0..+0x2a8,
// 0xa60 bytes each, key u32 at +0), matched as the game matches it: low byte and high word.
inline uint8_t *pu_record(uint8_t *begin, uint8_t *end, uint32_t driver)
{
    if (!begin || end <= begin || size_t(end - begin) % kPuRecordSize || size_t(end - begin) / kPuRecordSize > 256)
        return nullptr;
    for (uint8_t *r = begin; r < end; r += kPuRecordSize) {
        uint32_t key = at<uint32_t>(r, 0);
        if (uint8_t(key) == uint8_t(driver) && uint16_t(key >> 16) == uint16_t(driver >> 16))
            return r;
    }
    return nullptr;
}

}
