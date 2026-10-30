// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <vector>

#include "pu_scope.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

template <class T> static void put(void *p, size_t off, T v)
{
    std::memcpy(static_cast<uint8_t *>(p) + off, &v, sizeof v);
}

int main()
{
    std::vector<uint8_t> rows(3 * f1990::kEntrantSize);
    struct { uint8_t lo; uint16_t hi; uint16_t vehicle; } seats[] = {{7, 0x0123, 513}, {8, 0x0123, 526}, {7, 0x0456, 120}};
    for (int i = 0; i < 3; i++) {
        uint8_t *r = rows.data() + i * f1990::kEntrantSize;
        put<uint8_t>(r, 0x50, seats[i].lo);
        put<uint16_t>(r, 0x52, seats[i].hi);
        put<uint32_t>(r, 0x5c, (uint32_t(seats[i].vehicle) << 16) | 0x0005);
    }
    const uint8_t *b = rows.data(), *e = rows.data() + rows.size();
    check("finds the first driver's vehicle", f1990::driver_vehicle(b, e, 0x01230007) == 513);
    check("finds the second driver's vehicle", f1990::driver_vehicle(b, e, 0x01230008) == 526);
    check("matches both id halves", f1990::driver_vehicle(b, e, 0x04560007) == 120);
    check("ignores the middle byte of the key", f1990::driver_vehicle(b, e, 0x0123ff07) == 513);
    check("misses an unknown driver", f1990::driver_vehicle(b, e, 0x01230009) == f1990::kNoVehicle);
    check("rejects a partial row vector", f1990::driver_vehicle(b, e - 1, 0x01230007) == f1990::kNoVehicle);

    f1990::VehicleSet set;
    set.add(513);
    set.add(526);
    set.add(513);
    set.add(-1);
    set.add(70000);
    check("counts each vehicle once", set.count == 2);
    check("has the 1990 vehicles", set.has(513) && set.has(526));
    check("has no stock vehicle", !set.has(120) && !set.has(f1990::kNoVehicle));
    check("knows the 1990 packages",
          f1990::is_1990_package("f1_1990") && f1990::is_1990_package("f1_1990b") && !f1990::is_1990_package("f1_2026") &&
              !f1990::is_1990_package(nullptr));

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
