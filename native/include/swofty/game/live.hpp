#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

struct ControllerSystem {
    uint8_t _pad_0x0[0x30];
    ControllerSet **sets; // L2 experimental
    uint8_t _pad_0x38[0x58];
    void *vehicle_manager; // L2 experimental
    uint8_t _pad_0x98[0x2cc];
    uint8_t active; // L2 experimental
    uint8_t sets_built; // L2 experimental
};
static_assert(offsetof(ControllerSystem, sets) == 0x30);
static_assert(offsetof(ControllerSystem, vehicle_manager) == 0x90);
static_assert(offsetof(ControllerSystem, active) == 0x364);
static_assert(offsetof(ControllerSystem, sets_built) == 0x365);

struct GameManagerLive {
    uint8_t _pad_0x0[0x30];
    NeRobinHoodMapU64 *systems_map; // L2 experimental
    uint8_t _pad_0x38[0x298];
    GameVehicleProgressSystem *progress_system; // L2 experimental
};
static_assert(offsetof(GameManagerLive, systems_map) == 0x30);
static_assert(offsetof(GameManagerLive, progress_system) == 0x2d0);

struct GameVehicleProgressSystem {
    uint8_t _pad_0x0[0x38];
    void **entries_begin; // L2 experimental
    void **entries_end; // L2 experimental
};
static_assert(offsetof(GameVehicleProgressSystem, entries_begin) == 0x38);
static_assert(offsetof(GameVehicleProgressSystem, entries_end) == 0x40);

struct NeRobinHoodMapU64 {
    uint8_t _pad_0x0[0x10];
    uint64_t hash_seed; // L2 experimental
    uint8_t *slots; // L2 experimental
    uint8_t *meta; // L2 experimental
    uint64_t count; // L2 experimental
    uint64_t mask; // L2 experimental
    uint64_t max_count; // L2 experimental
    uint32_t step; // L2 experimental
    uint32_t shift; // L2 experimental
};
static_assert(offsetof(NeRobinHoodMapU64, hash_seed) == 0x10);
static_assert(offsetof(NeRobinHoodMapU64, slots) == 0x18);
static_assert(offsetof(NeRobinHoodMapU64, meta) == 0x20);
static_assert(offsetof(NeRobinHoodMapU64, count) == 0x28);
static_assert(offsetof(NeRobinHoodMapU64, mask) == 0x30);
static_assert(offsetof(NeRobinHoodMapU64, max_count) == 0x38);
static_assert(offsetof(NeRobinHoodMapU64, step) == 0x40);
static_assert(offsetof(NeRobinHoodMapU64, shift) == 0x44);

struct VehicleProgressEntry {
    uint8_t _pad_0x0[0x38];
    uint16_t lap_a; // L2 experimental
    uint8_t _pad_0x3a[0x6];
    uint16_t lap_b; // L2 experimental
    uint8_t _pad_0x42[0x6];
    uint16_t progress_lap_a; // L2 experimental
    uint8_t _pad_0x4a[0x6];
    uint16_t progress_lap_b; // L2 experimental
    uint8_t _pad_0x52[0x6];
    int16_t progress_lap_delta; // L2 experimental
    uint8_t _pad_0x5a[0x3e];
    uint32_t total_a; // L2 experimental
    uint8_t _pad_0x9c[0x4];
    uint32_t total_b; // L2 experimental
};
static_assert(offsetof(VehicleProgressEntry, lap_a) == 0x38);
static_assert(offsetof(VehicleProgressEntry, lap_b) == 0x40);
static_assert(offsetof(VehicleProgressEntry, progress_lap_a) == 0x48);
static_assert(offsetof(VehicleProgressEntry, progress_lap_b) == 0x50);
static_assert(offsetof(VehicleProgressEntry, progress_lap_delta) == 0x58);
static_assert(offsetof(VehicleProgressEntry, total_a) == 0x98);
static_assert(offsetof(VehicleProgressEntry, total_b) == 0xa0);

}
