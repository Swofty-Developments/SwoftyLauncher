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

}
