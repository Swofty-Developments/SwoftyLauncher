#include "build_logic.hpp"
#include "live_car_logic.hpp"

#include <cstdint>
#include <cstdio>
#include <initializer_list>

static int failures;
#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

namespace build = runtime::build;

struct Exe {
    uint32_t stamp, size;
};

constexpr Exe KB{1789118616u, 0x18518000}, KB_PACKED{1789118616u, 0x6a00000}, EA{1789128892u, 0x155dc000},
    STEAM_145{1794312636u, 0x18434000}, EA_145{1794315435u, 0x1615e000};

static live_car::Fields fields(Exe e)
{
    return live_car::fields_of(build::kb(e.stamp, e.size), build::ported(e.stamp, e.size));
}

int main()
{
    for (Exe e : {KB, KB_PACKED, EA, STEAM_145}) {
        CHECK(build::verified(e.stamp, e.size));
        CHECK(build::ported(e.stamp, e.size));
    }
    CHECK(!build::verified(EA_145.stamp, EA_145.size));
    CHECK(build::ported(EA_145.stamp, EA_145.size));
    CHECK(build::ea_1_0_145(EA_145.stamp, EA_145.size) && !build::ea_1_0_145(STEAM_145.stamp, STEAM_145.size));

    CHECK(!build::ported(EA_145.stamp, STEAM_145.size));
    CHECK(!build::ported(STEAM_145.stamp, EA_145.size));
    CHECK(!build::ported(EA.stamp, EA_145.size));
    CHECK(!build::ported(EA_145.stamp + 1, EA_145.size));
    CHECK(!build::ported(KB.stamp, 0x5000000));
    CHECK(!build::ported(0, 0));

    CHECK(build::kb(KB.stamp, KB.size) && build::kb(KB_PACKED.stamp, KB_PACKED.size));
    for (Exe e : {EA, STEAM_145, EA_145})
        CHECK(!build::kb(e.stamp, e.size));

    {
        live_car::Fields kb = fields(KB), ea = fields(EA), steam = fields(STEAM_145), ea145 = fields(EA_145),
                         other = fields({1794000000u, EA_145.size});
        CHECK(kb.table && kb.drs && kb.wheels && kb.surface && kb.wetness && kb.orient && kb.angvel);
        for (const live_car::Fields &f : {ea, steam, ea145}) {
            CHECK(f.table && f.drs);
            CHECK(!f.wheels && !f.surface && !f.wetness && !f.orient && !f.angvel);
        }
        CHECK(!other.table && !other.drs && !other.wheels);
    }

    if (failures)
        return 1;
    std::printf("build gates ok\n");
    return 0;
}
