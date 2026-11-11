#pragma once
#include <swofty/game/base.hpp>
#include <swofty/game/car.hpp>

namespace swofty::game {

struct CarBodyActor {
    uint8_t _pad_0x0[0x18];
    CarBodyState *state; // L2 experimental
};
static_assert(offsetof(CarBodyActor, state) == 0x18);

struct CarBodyState {
    uint8_t _pad_0x0[0x1c0];
    float position[3]; // L2 experimental
    uint8_t _pad_0x1cc[0x54];
    float velocity[3]; // L2 experimental
};
static_assert(offsetof(CarBodyState, position) == 0x1c0);
static_assert(offsetof(CarBodyState, velocity) == 0x220);

struct CarChassisLive {
    uint8_t _pad_0x0[0x320];
    float g_lat; // L2 experimental
    float g_long; // L2 experimental
    float g_vert; // L2 experimental
    uint8_t _pad_0x32c[0x14];
    CarBodyActor *body; // L2 experimental
};
static_assert(offsetof(CarChassisLive, g_lat) == 0x320);
static_assert(offsetof(CarChassisLive, g_long) == 0x324);
static_assert(offsetof(CarChassisLive, g_vert) == 0x328);
static_assert(offsetof(CarChassisLive, body) == 0x340);

struct CarLiveInputs {
    uint8_t _pad_0x0[0x188];
    float throttle; // L2 experimental
    float clutch; // L2 experimental
    float brake; // L2 experimental
    uint8_t _pad_0x194[0x90];
    uint8_t input_flags; // L2 experimental
};
static_assert(offsetof(CarLiveInputs, throttle) == 0x188);
static_assert(offsetof(CarLiveInputs, clutch) == 0x18c);
static_assert(offsetof(CarLiveInputs, brake) == 0x190);
static_assert(offsetof(CarLiveInputs, input_flags) == 0x224);

struct CarSteeringLive {
    uint8_t _pad_0x0[0xe8];
    CarObfFloat steering_angle; // L2 experimental
};
static_assert(offsetof(CarSteeringLive, steering_angle) == 0xe8);

struct DriveThroughObject {
    uint8_t _pad_0x0[0x68];
    uint8_t *records_begin; // L2 experimental
    uint8_t *records_end; // L2 experimental
};
static_assert(offsetof(DriveThroughObject, records_begin) == 0x68);
static_assert(offsetof(DriveThroughObject, records_end) == 0x70);

struct MarshalZone {
    MarshalZoneDef *definition; // L2 experimental
    uint8_t _pad_0x8[0xc];
    uint8_t flag; // L2 experimental
};
static_assert(offsetof(MarshalZone, definition) == 0x0);
static_assert(offsetof(MarshalZone, flag) == 0x14);

struct MarshalZoneDef {
    uint8_t _pad_0x0[0x10];
    float start_fraction; // L2 experimental
};
static_assert(offsetof(MarshalZoneDef, start_fraction) == 0x10);

struct MarshalZoneMonitor {
    uint8_t _pad_0x0[0x88];
    MarshalZoneSlot *zones_begin; // L2 experimental
    MarshalZoneSlot *zones_end; // L2 experimental
};
static_assert(offsetof(MarshalZoneMonitor, zones_begin) == 0x88);
static_assert(offsetof(MarshalZoneMonitor, zones_end) == 0x90);

struct MarshalZoneSlot {
    uint8_t _pad_0x0[0x8];
    MarshalZone *zone; // L2 experimental
};
static_assert(offsetof(MarshalZoneSlot, zone) == 0x8);

struct PenaltyLog {
    uint8_t _pad_0x0[0x48];
    PenaltyRecord *records_begin; // L2 experimental
    PenaltyRecord *records_end; // L2 experimental
};
static_assert(offsetof(PenaltyLog, records_begin) == 0x48);
static_assert(offsetof(PenaltyLog, records_end) == 0x50);

struct PenaltyRecord {
    uint8_t _pad_0x0[0x8];
    uint8_t vehicle_id; // L2 experimental
    uint8_t penalty_type; // L2 experimental
    uint8_t infringement; // L2 experimental
};
static_assert(offsetof(PenaltyRecord, vehicle_id) == 0x8);
static_assert(offsetof(PenaltyRecord, penalty_type) == 0x9);
static_assert(offsetof(PenaltyRecord, infringement) == 0xa);

struct PitLaneCarSlot {
    PitLaneCarState *state; // L2 experimental
};
static_assert(offsetof(PitLaneCarSlot, state) == 0x0);

struct PitLaneCarState {
    uint8_t _pad_0x0[0x4];
    uint32_t state; // L2 experimental
    uint8_t _pad_0x8[0xc];
    uint32_t pit_stops; // L2 experimental
};
static_assert(offsetof(PitLaneCarState, state) == 0x4);
static_assert(offsetof(PitLaneCarState, pit_stops) == 0x14);

struct PitLaneSystem {
    uint8_t _pad_0x0[0x68];
    PitLaneCarSlot car_slots[27]; // L2 experimental
};
static_assert(offsetof(PitLaneSystem, car_slots) == 0x68);

struct RelativeGapEntry {
    uint8_t _pad_0x0[0x58];
    int64_t leader_gap_ms; // L2 experimental
    uint8_t _pad_0x60[0x8];
    uint8_t leader_gap_valid; // L2 experimental
    uint8_t _pad_0x69[0x7];
    int64_t interval_ms; // L2 experimental
    uint8_t _pad_0x78[0x8];
    uint8_t interval_valid; // L2 experimental
    uint8_t _pad_0x81[0xf];
    uint8_t vehicle_id; // L2 experimental
};
static_assert(offsetof(RelativeGapEntry, leader_gap_ms) == 0x58);
static_assert(offsetof(RelativeGapEntry, leader_gap_valid) == 0x68);
static_assert(offsetof(RelativeGapEntry, interval_ms) == 0x70);
static_assert(offsetof(RelativeGapEntry, interval_valid) == 0x80);
static_assert(offsetof(RelativeGapEntry, vehicle_id) == 0x90);

struct RelativeGapHelper {
    uint8_t _pad_0x0[0x30];
    RelativeGapEntry *entries_begin; // L2 experimental
    RelativeGapEntry *entries_end; // L2 experimental
    uint8_t _pad_0x40[0x90];
    PitLaneSystem *pit_lane; // L2 experimental
};
static_assert(offsetof(RelativeGapHelper, entries_begin) == 0x30);
static_assert(offsetof(RelativeGapHelper, entries_end) == 0x38);
static_assert(offsetof(RelativeGapHelper, pit_lane) == 0xd0);

struct SessionSystem {
    uint8_t _pad_0x0[0x148];
    SessionVehicleEntry **entries_begin; // L2 experimental
    SessionVehicleEntry **entries_end; // L2 experimental
};
static_assert(offsetof(SessionSystem, entries_begin) == 0x148);
static_assert(offsetof(SessionSystem, entries_end) == 0x150);

struct SessionVehicleEntry {
    uint8_t _pad_0x0[0x18];
    uint8_t status; // L2 experimental
    uint8_t _pad_0x19[0x1f];
    uint64_t time_penalty_a; // L2 experimental
    uint64_t time_penalty_b; // L2 experimental
    uint32_t position_a; // L2 experimental
    uint8_t _pad_0x4c[0x4];
    uint32_t position_b; // L2 experimental
    uint8_t _pad_0x54[0x3c];
    uint32_t grid_position_a; // L2 experimental
    uint8_t _pad_0x94[0x4];
    uint32_t grid_position_b; // L2 experimental
};
static_assert(offsetof(SessionVehicleEntry, status) == 0x18);
static_assert(offsetof(SessionVehicleEntry, time_penalty_a) == 0x38);
static_assert(offsetof(SessionVehicleEntry, time_penalty_b) == 0x40);
static_assert(offsetof(SessionVehicleEntry, position_a) == 0x48);
static_assert(offsetof(SessionVehicleEntry, position_b) == 0x50);
static_assert(offsetof(SessionVehicleEntry, grid_position_a) == 0x90);
static_assert(offsetof(SessionVehicleEntry, grid_position_b) == 0x98);

struct StopGoObject {
    uint8_t _pad_0x0[0x68];
    uint8_t *records_begin; // L2 experimental
    uint8_t *records_end; // L2 experimental
};
static_assert(offsetof(StopGoObject, records_begin) == 0x68);
static_assert(offsetof(StopGoObject, records_end) == 0x70);

struct VehicleHelperSystem {
    uint8_t _pad_0x0[0x568];
    DriveThroughObject *drive_through; // L2 experimental
    uint8_t _pad_0x570[0x8];
    StopGoObject *stop_go; // L2 experimental
};
static_assert(offsetof(VehicleHelperSystem, drive_through) == 0x568);
static_assert(offsetof(VehicleHelperSystem, stop_go) == 0x578);

}
