// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <vector>

#include "contract_target.hpp"

using f1990::RatingRange;

static int failures;

static void expect(const char *name, int32_t base, const std::vector<RatingRange> &rows, int32_t want)
{
    int32_t got = f1990::recommended_target(base, rows.data(), rows.data() + rows.size());
    std::printf("%s %s: %d", got == want ? "ok  " : "FAIL", name, got);
    if (got != want) {
        std::printf("  (want %d)", want);
        failures++;
    }
    std::printf("\n");
}

int main()
{
    std::vector<RatingRange> rows = {{0, 59, 5, 10, 4}, {60, 79, 5, 8, 3}, {80, 100, 3, 5, 2}};
    expect("first matching range", 55, rows, 59);
    expect("middle range", 72, rows, 75);
    expect("range bounds are inclusive", 80, rows, 82);
    expect("capped at 100", 99, rows, 100);
    expect("no rows keeps the rating", 70, {}, 70);
    expect("no matching row keeps the rating", 70, {{0, 10, 1, 1, 1}}, 70);
    expect("offset above the slider is clamped", 50, {{0, 100, 5, 2, 9}}, 52);
    expect("negative offset is clamped to the slider", 50, {{0, 100, 3, 5, -9}}, 47);
    expect("negative rating counts as 0", -4, rows, 4);
    if (failures)
        std::printf("%d failure(s)\n", failures);
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
