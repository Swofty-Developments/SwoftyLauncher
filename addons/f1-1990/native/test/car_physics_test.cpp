// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstring>
#include <vector>

#include "car_physics.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static void put(std::vector<uint8_t> &gb, size_t off, float v, uint32_t key)
{
    uint32_t bits;
    std::memcpy(&bits, &v, 4);
    uint32_t a = bits ^ key;
    std::memcpy(&gb[off], &a, 4);
    std::memcpy(&gb[off + 8], &key, 4);
}

int main()
{
    using namespace f1990::physics;
    std::vector<uint8_t> gb(0x200);
    put(gb, kGearboxFinalDrive, 4.9f, 0x1234abcd);
    const float six[6] = {2.9f, 2.1f, 1.6f, 1.3f, 1.1f, 0.95f};
    for (int i = 0; i < 6; i++)
        put(gb, kGearbox1st + size_t(i) * kGearStride, six[i], 0x9e3779b9u + uint32_t(i));
    for (int i = 6; i < 9; i++)
        put(gb, kGearbox1st + size_t(i) * kGearStride, 0.f, 0x55u + uint32_t(i));
    Gears g = gears(gb.data());
    check("six ratios counted", g.count == 6);
    check("first ratio decoded", g.ratio[0] == 2.9f);
    check("sixth ratio decoded", g.ratio[5] == 0.95f);
    check("final drive decoded", g.final_drive == 4.9f);
    put(gb, kGearbox1st + 6 * kGearStride, 0.9f, 7);
    put(gb, kGearbox1st + 7 * kGearStride, 0.8f, 8);
    check("eight ratios counted", gears(gb.data()).count == 8);

    const char vtf[] = "Chassis\0HandlingVersion\0handling2025\0TyrePhysicsGroup\0F1_1990P\0GearRatio1st\0GearRatio6th\0";
    auto *b = reinterpret_cast<const uint8_t *>(vtf);
    check("1990 marker found", is_1990_vtf(b, sizeof vtf));
    check("1990 marker missing", !is_1990_vtf(reinterpret_cast<const uint8_t *>("TyrePhysicsGroup\0F1_2026\0"), 26));
    check("gear names counted", gear_names(b, sizeof vtf) == 2);
    check("null buffer", !is_1990_vtf(nullptr, 10) && gear_names(nullptr, 0) == 0);

    check("clears for a 1990 asset in equal performance", clears_equal_performance(true, true, 1, true));
    check("keeps a stock asset", !clears_equal_performance(true, false, 1, true));
    check("keeps the flag off", !clears_equal_performance(true, true, 0, true));
    check("keeps without a base asset", !clears_equal_performance(true, true, 1, false));
    check("option off", !clears_equal_performance(false, true, 1, true));
    return failures ? 1 : 0;
}
