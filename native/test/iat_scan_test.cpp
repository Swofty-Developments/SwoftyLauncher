#include "core/runtime/iat_scan_logic.hpp"

#include <cstdio>
#include <vector>

namespace scan = runtime::iat_scan;

static int failures;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                                   \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

constexpr uint64_t LO = 0x140000000, HI = LO + 0x1615e000;
constexpr uint64_t SENDTO = 0x6fffff8e23a0;

static bool at(const std::vector<uint64_t> &q, size_t i) { return scan::in_table(q.data(), q.size(), i, LO, HI); }

int main()
{
    CHECK(scan::module_pointer(SENDTO, LO, HI));
    CHECK(!scan::module_pointer(0, LO, HI));
    CHECK(!scan::module_pointer(0x5000, LO, HI));
    CHECK(!scan::module_pointer(LO + 0x1000, LO, HI));
    CHECK(!scan::module_pointer(LO, LO, HI));
    CHECK(!scan::module_pointer(HI - 8, LO, HI));
    CHECK(scan::module_pointer(HI, LO, HI));
    CHECK(!scan::module_pointer(0xffff800000000000ull, LO, HI));

    std::vector<uint64_t> second = {0x6fffffb9e2a0, 0x6fffffebc7b0, 0x6fffffecb2a0, 0x6fffffebe730, 0x6fffffedc010,
                                    0x6fffffedc180, 0x6ffffff3e530, 0x6fffff8e16d0, 0x6fffff8e11f0, 0x6fffff8d6dc0,
                                    0x6fffff8d9700, 0x6fffff8e1980, 0x6fffff8e2340, 0x6fffff8db060, SENDTO,
                                    0x6fffff8e2890, 0x6fffff8e3a20, 0x6fffff8db390, 0x6fffff8db4e0, 0x6fffff8dee40,
                                    0x6fffff8e0250, 0x6fffff8e11e0};
    CHECK(second.size() == 22);
    CHECK(at(second, 14));
    CHECK(at(second, 0));
    CHECK(at(second, 21));
    CHECK(!at(second, 22));

    std::vector<uint64_t> first = {
        0x6fffff8d5b40, 0,      0x6fffff8d5c10, 0x6fffff8d62e0, 0x6fffff8d6dc0, 0x6fffff8d6dd0, 0x6fffff8d76e0, 0x6fffff8d8b80,
        0x6fffff8d8c60, SENDTO, 0x6fffff8d8ec0};
    CHECK(at(first, 9));
    first[1] = LO + 0x2000;
    CHECK(at(first, 9));
    first[8] = 0;
    CHECK(!at(first, 9));
    first[8] = 0x6fffff8d8c60;
    first[7] = 0;
    CHECK(!at(first, 9));

    std::vector<uint64_t> stray = {0, 0, SENDTO, 0x6fffff8e11e0, 0x6fffff8e0250, 0, 0};
    CHECK(!at(stray, 2));

    std::vector<uint64_t> exact(scan::MIN_RUN, SENDTO);
    CHECK(at(exact, 0));
    CHECK(at(exact, scan::MIN_RUN - 1));
    exact.pop_back();
    CHECK(!at(exact, 0));

    if (failures)
        return 1;
    printf("ok\n");
    return 0;
}
