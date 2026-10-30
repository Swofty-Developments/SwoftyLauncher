// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cmath>
#include <cstdio>

#include "race_engine.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static float rad(float rpm) { return rpm * f1990::race_engine::kRpmToRadS; }

int main()
{
    using namespace f1990::race_engine;
    float lim = rad(13500.f);
    check("power scaled below the race limit", std::fabs(torque_scale(rad(11000.f), lim) - kPower) < 1e-6f);
    check("full race power 60 rpm under the race limit", std::fabs(torque_scale(rad(13140.f), lim) - kPower) < 1e-3f);
    check("half way down the ramp", std::fabs(torque_scale(rad(13170.f), lim) - kPower * 0.5f) < 1e-3f);
    check("no torque at the race limit", torque_scale(rad(13200.f), lim) == 0.f);
    check("no torque past the race limit", torque_scale(rad(13450.f), lim) == 0.f);
    check("no limiter read: power only", std::fabs(torque_scale(rad(20000.f), 0.f) - kPower) < 1e-6f);
    check("nan speed: power only", std::fabs(torque_scale(NAN, lim) - kPower) < 1e-6f);
    return failures ? 1 : 0;
}
