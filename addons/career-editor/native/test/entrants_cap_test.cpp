// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Host unit test for the entrant vector bound: a career roster with free agents runs past 128 rows
// (the largest seen is 210), and the vector must still read.
#include <cstdio>
#include <vector>

#include "components.hpp"

namespace ce::comp {
bool readable(uintptr_t, size_t) { return true; }
}

static int fails = 0;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++fails; } } while (0)

struct Row { unsigned char bytes[0x88]; };

int main() {
    for (size_t rows : {24u, 128u, 131u, 210u, 256u}) {
        std::vector<Row> v(rows);
        size_t n = 0;
        CHECK(ce::comp::array(v.data(), v.data() + rows, ce::comp::kMaxEntrants, n));
        CHECK(n == rows);
    }
    std::vector<Row> big(ce::comp::kMaxEntrants + 1);
    size_t n = 0;
    CHECK(!ce::comp::array(big.data(), big.data() + big.size(), ce::comp::kMaxEntrants, n));
    CHECK(ce::comp::kMaxEntrants >= 256);
    if (fails) return 1;
    std::printf("ok\n");
    return 0;
}
