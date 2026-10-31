// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string_view>

#include "weekend.hpp"

namespace f1990::physics {

constexpr size_t kGearbox1st = 0x58;
constexpr size_t kGearboxFinalDrive = 0x28;
constexpr size_t kGearStride = 0x10;
constexpr int kGears = 9;
constexpr size_t kEqualPerformance = 0x12c;
constexpr size_t kBaseAsset = 0x48;
constexpr size_t kAssetResource = 0x28;
constexpr std::string_view kMarker = "F1_1990";

inline float obf_float(const uint8_t *pair)
{
    uint32_t bits = at<uint32_t>(pair, 0) ^ at<uint32_t>(pair, 8);
    float f;
    std::memcpy(&f, &bits, sizeof f);
    return f;
}

struct Gears {
    float ratio[kGears] = {};
    float final_drive = 0.f;
    int count = 0;
};

inline Gears gears(const uint8_t *gearbox)
{
    Gears g;
    g.final_drive = obf_float(gearbox + kGearboxFinalDrive);
    for (int i = 0; i < kGears; i++)
        g.ratio[i] = obf_float(gearbox + kGearbox1st + size_t(i) * kGearStride);
    while (g.count < kGears && g.ratio[g.count] > 0.01f)
        g.count++;
    return g;
}

inline bool contains(const uint8_t *data, size_t size, std::string_view needle)
{
    if (!data || size < needle.size())
        return false;
    auto *b = reinterpret_cast<const char *>(data);
    return std::search(b, b + size, needle.begin(), needle.end()) != b + size;
}

inline int gear_names(const uint8_t *data, size_t size)
{
    static constexpr std::string_view names[] = {"GearRatio1st", "GearRatio2nd", "GearRatio3rd",
                                                 "GearRatio4th", "GearRatio5th", "GearRatio6th",
                                                 "GearRatio7th", "GearRatio8th", "GearRatio9th"};
    int n = 0;
    for (auto name : names)
        n += contains(data, size, name);
    return n;
}

inline bool is_1990_vtf(const uint8_t *data, size_t size) { return contains(data, size, kMarker); }

inline bool clears_equal_performance(bool own_handling, bool vtf_is_1990, uint32_t flag, bool has_base)
{
    return own_handling && vtf_is_1990 && flag != 0 && has_base;
}

}
