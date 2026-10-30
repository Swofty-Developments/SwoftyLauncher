// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

namespace f1990::race_engine {

constexpr float kPower = 0.9725f;
constexpr float kLimiterDropRpm = 300.0f;
constexpr float kRampRpm = 60.0f;
constexpr float kRpmToRadS = 0.10471976f;

inline float torque_scale(float rad_s, float limiter_rad_s)
{
    float k = kPower;
    float cut = limiter_rad_s - kLimiterDropRpm * kRpmToRadS;
    float ramp = kRampRpm * kRpmToRadS;
    if (!(cut > 0.f) || !(rad_s > cut - ramp))
        return k;
    float f = (cut - rad_s) / ramp;
    return k * (f > 0.f ? f : 0.f);
}

}
