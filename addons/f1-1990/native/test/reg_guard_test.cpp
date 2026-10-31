// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstring>

#include "reg_guard.hpp"

static int g_fail;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++g_fail; } } while (0)

using f1990::BodyRegistry;

int main()
{
    CHECK(!BodyRegistry::full(0));
    CHECK(!BodyRegistry::full(25));
    CHECK(BodyRegistry::full(26));
    CHECK(BodyRegistry::full(27));
    CHECK(BodyRegistry::full(0xffffffffu));

    uint8_t reg[0x600] = {};
    for (uint32_t cars = 0; cars < 30; ++cars) {
        std::memcpy(reg + BodyRegistry::kCount, &cars, sizeof cars);
        CHECK(BodyRegistry::count(reg) == cars);
        CHECK(BodyRegistry::full(BodyRegistry::count(reg)) == (cars >= 26));
    }

    uint32_t registered = 0, skipped = 0, count = 0;
    for (int car = 0; car < 27; ++car) {
        if (BodyRegistry::full(count)) {
            ++skipped;
            continue;
        }
        ++count;
        ++registered;
    }
    CHECK(registered == 26 && skipped == 1 && count == 26);

    std::memset(reg, 0, sizeof reg);
    CHECK(!BodyRegistry::scheduled(reg));
    uint64_t sched = 0x1000;
    std::memcpy(reg + BodyRegistry::kScheduler, &sched, sizeof sched);
    CHECK(BodyRegistry::scheduled(reg));

    if (!g_fail)
        std::puts("reg_guard_test ok");
    return g_fail ? 1 : 0;
}
