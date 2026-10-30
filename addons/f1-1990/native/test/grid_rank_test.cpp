// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <vector>

#include "grid_rank.hpp"

using f1990::Driver;
using f1990::kUnclassified;

static int failures;

static void expect(const char *name, const std::vector<Driver> &in, const std::vector<int32_t> &want)
{
    std::vector<int32_t> got = f1990::rank(in);
    bool ok = got == want;
    std::printf("%s %s:", ok ? "ok  " : "FAIL", name);
    for (int32_t p : got)
        std::printf(" %d", p);
    if (!ok) {
        std::printf("  (want");
        for (int32_t p : want)
            std::printf(" %d", p);
        std::printf(")");
        failures++;
    }
    std::printf("\n");
}

int main()
{
    expect("saturday only keeps the order",
           {{1, 0, 0.f, 80.0f}, {2, 1, 0.f, 80.5f}, {3, 2, 0.f, 81.0f}}, {0, 1, 2});
    expect("friday lap beats saturday",
           {{1, 0, 82.0f, 80.0f}, {2, 1, 79.5f, 80.5f}, {3, 2, 0.f, 81.0f}}, {1, 0, 2});
    expect("friday only driver is classified",
           {{1, 0, 0.f, 80.0f}, {2, kUnclassified, 79.0f, 0.f}, {3, 1, 0.f, 81.0f}}, {1, 0, 2});
    expect("no time goes to the back in saturday order",
           {{1, 2, 0.f, 0.f}, {2, 0, 0.f, 80.0f}, {3, 1, 0.f, 0.f}, {4, 3, 81.0f, 0.f}}, {3, 0, 2, 1});
    expect("no time and unclassified stays unclassified",
           {{1, 0, 0.f, 80.0f}, {2, kUnclassified, 0.f, 0.f}}, {0, kUnclassified});
    expect("tie keeps saturday order",
           {{1, 1, 80.0f, 0.f}, {2, 0, 0.f, 80.0f}}, {1, 0});
    expect("one-based positions stay one-based",
           {{1, 1, 0.f, 80.0f}, {2, 2, 79.0f, 81.0f}, {3, 3, 0.f, 82.0f}}, {2, 1, 3});
    expect("non-finite and negative times are no time",
           {{1, 0, -1.f, 80.0f}, {2, 1, 1.f / 0.f, 0.f}}, {0, 1});
    std::vector<Driver> grid;
    for (uint32_t i = 0; i < 26; i++)
        grid.push_back({i, int32_t(i), 90.f - 0.1f * i, i % 3 ? 95.f + 0.1f * i : 0.f});
    std::vector<int32_t> want(26);
    for (int i = 0; i < 26; i++)
        want[i] = 25 - i;
    expect("26 cars, friday faster in reverse", grid, want);
    if (failures)
        std::printf("%d failure(s)\n", failures);
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
