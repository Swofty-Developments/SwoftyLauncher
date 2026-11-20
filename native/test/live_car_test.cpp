#include "core/live/live_car_logic.hpp"
#include "core/render/postfx_logic.hpp"

#include <cstdio>

static int failures;
#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

using namespace render::postfx;

int main()
{
    const int expect[13] = {0, 1, 0, 4, 4, 4, 5, 3, 0, 0, 0, 2, -1};
    for (int id = 0; id < 13; id++)
        CHECK(surface_value(id) == expect[id]);
    CHECK(surface_value(-1) == -1 && surface_value(13) == -1 && surface_value(255) == -1);
    CHECK(signal_value_name(S_CAR_SURFACE, size_t(surface_value(1))) == "kerb");
    CHECK(signal_value_name(S_CAR_SURFACE, size_t(surface_value(11))) == "rumble");
    CHECK(signal_value_name(S_CAR_SURFACE, size_t(surface_value(7))) == "grass");
    CHECK(signal_value_name(S_CAR_SURFACE, size_t(surface_value(3))) == "gravel");
    CHECK(signal_value_name(S_CAR_SURFACE, size_t(surface_value(6))) == "sand");

    const uint8_t game[4] = {10, 11, 12, 13};
    uint8_t out[4];
    wheels_contract_order(game, out);
    CHECK(out[0] == 12 && out[1] == 13 && out[2] == 10 && out[3] == 11);

    for (int s = S_CAR_SPEED; s <= S_CAR_SURFACE; s++)
        CHECK(SIGNALS[s].in_game);
    CHECK(SIGNALS[S_SESSION_WETNESS].in_game && SIGNALS[S_CAR_WETNESS].in_game);
    for (int s : {S_CAR_IMPACT, S_CAR_DAMAGE, S_CAR_IN_PIT_LANE, S_CAR_OFF_TRACK, S_SESSION_FLAG, S_SESSION_SAFETY_CAR,
                  S_SESSION_WEATHER, S_SESSION_FLASHBACK, S_SESSION_CHEQUERED})
        CHECK(!SIGNALS[s].in_game);

    {
        namespace bp = runtime::build_proof;
        live_car::Fields none = live_car::fields_of(bp::Build::unknown), ea = live_car::fields_of(bp::Build::ea),
                         steam = live_car::fields_of(bp::Build::kb), port = live_car::fields_of(bp::Build::steam_1_0_145);
        CHECK(!none.table && !none.wetness && !none.surface && !none.orient && !none.angvel && !none.wheels && !none.drs);
        CHECK(ea.table && ea.drs);
        CHECK(!ea.wetness && !ea.surface && !ea.orient && !ea.angvel && !ea.wheels);
        CHECK(steam.table && steam.wetness && steam.surface && steam.orient && steam.angvel && steam.wheels && steam.drs);
        CHECK(port.table && port.drs && port.wheels && port.wetness);
        CHECK(!port.surface && !port.orient && !port.angvel);
        live_car::Fields ea145 = live_car::fields_of(bp::Build::ea_1_0_145);
        CHECK(ea145.wheels && ea145.wetness && ea145.table && ea145.drs);
        CHECK(!ea145.surface && !ea145.orient && !ea145.angvel);
    }

    if (failures)
        return 1;
    std::printf("live car mapping ok\n");
    return 0;
}
