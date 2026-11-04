#pragma once

#include <cstdint>

namespace live_car {

struct LiveCar {
    bool has_speed = false, has_pedals = false, has_steer = false, has_gear = false, has_rpm = false, has_g = false,
         has_surface = false;
    float speed_kmh = 0, throttle = 0, brake = 0, steer = 0, rpm = 0, max_rpm = 0, g_lateral = 0, g_long = 0;
    int gear = 0;
    uint8_t surface[4] = {12, 12, 12, 12};
    int player = -1;
    bool ai = false;
};

bool read(LiveCar &out);

}
