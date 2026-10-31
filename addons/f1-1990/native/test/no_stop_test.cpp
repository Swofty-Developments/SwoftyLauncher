// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstring>

#include "no_stop.hpp"

static int g_fail;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++g_fail; } } while (0)

int main()
{
    uint8_t mode[0x100];
    for (size_t i = 0; i < sizeof mode; ++i)
        mode[i] = uint8_t(i * 7 + 1);
    mode[0x88] = 1;
    mode[0x89] = 1;
    mode[0x8a] = 1;
    uint8_t before[sizeof mode];
    std::memcpy(before, mode, sizeof mode);

    f1990::StopRules r;
    CHECK(r.relax(mode));
    CHECK(mode[0x88] == 0 && mode[0x89] == 0);
    CHECK(mode[0x8a] == 1);
    for (size_t i = 0x8b; i < 0xb2; ++i)
        CHECK(mode[i] == 0);
    CHECK(mode[0x87] == before[0x87] && mode[0xb2] == before[0xb2]);
    CHECK(!r.relax(mode));

    CHECK(r.restore(mode));
    CHECK(std::memcmp(mode, before, sizeof mode) == 0);
    CHECK(!r.restore(mode));

    if (!g_fail)
        std::puts("no_stop_test ok");
    return g_fail ? 1 : 0;
}
