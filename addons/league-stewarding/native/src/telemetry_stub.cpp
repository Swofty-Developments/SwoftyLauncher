// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Stands in for telemetry.cpp in a tree that does not have it: build.sh compiles this file only when
// telemetry.cpp is absent.

#include "telemetry.hpp"

namespace steward {

bool telemetry_start(uint16_t)
{
    return false;
}

void telemetry_stop() {}

Snapshot telemetry_snapshot()
{
    return {};
}

}
