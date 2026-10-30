// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <bitset>
#include <cstdint>
#include <cstring>

#include "weekend.hpp"

namespace f1990 {

constexpr size_t kEntrantSize = 0x88;
constexpr size_t kEntrantMax = 256;
constexpr uint32_t kNoVehicle = 0x3fff;

// The vehicle stat id of the driver's career entrant row (rows of 0x88 bytes: id_lo +0x50, id_hi
// +0x52, vehicle ref +0x5c with the stat id in the high word), matched the way the engine
// management matches its driver keys (key low byte = id_lo, key high word = id_hi).
inline uint32_t driver_vehicle(const uint8_t *begin, const uint8_t *end, uint32_t driver)
{
    if (!begin || end < begin || size_t(end - begin) % kEntrantSize || size_t(end - begin) / kEntrantSize > kEntrantMax)
        return kNoVehicle;
    for (const uint8_t *r = begin; r < end; r += kEntrantSize)
        if (at<uint8_t>(r, 0x50) == uint8_t(driver) && at<uint16_t>(r, 0x52) == uint16_t(driver >> 16))
            return at<uint32_t>(r, 0x5c) >> 16;
    return kNoVehicle;
}

// Vehicle stat ids whose vehicle_package is a 1990 package.
struct VehicleSet {
    std::bitset<0x10000> ids;
    size_t count = 0;

    void add(int64_t stat_id)
    {
        if (stat_id >= 0 && stat_id < int64_t(ids.size()) && !ids.test(size_t(stat_id))) {
            ids.set(size_t(stat_id));
            count++;
        }
    }

    bool has(uint32_t stat_id) const { return stat_id < ids.size() && ids.test(stat_id); }
};

inline bool is_1990_package(const char *package)
{
    return package && (!std::strcmp(package, "f1_1990") || !std::strcmp(package, "f1_1990b"));
}

}
