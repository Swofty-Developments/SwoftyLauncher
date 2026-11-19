// Host unit test of the live car ABI: SwoftyLiveCar's size, alignment and every field's offset, as addons built
// against an earlier swofty_api.h read them. A field added in the reserved space must leave all of these unchanged;
// the new fields' offsets are listed after the frozen ones. Also the pit lane reads over those fields
// (swofty/systems/pitlane.hpp).
#include <swofty_api.h>
#include <swofty/systems/pitlane.hpp>

#include <cstddef>
#include <cstdio>
#include <string>

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

#define FROZEN                                                                                                                   \
    X(version, 0)                                                                                                                \
    X(size, 4)                                                                                                                   \
    X(have, 8)                                                                                                                   \
    X(vehicle, 16)                                                                                                               \
    X(participant, 20)                                                                                                           \
    X(flags, 24)                                                                                                                 \
    X(ers_class, 25)                                                                                                             \
    X(pad, 26)                                                                                                                   \
    X(speed, 28)                                                                                                                 \
    X(rpm, 32)                                                                                                                   \
    X(rpm_limit, 36)                                                                                                             \
    X(gear, 40)                                                                                                                  \
    X(throttle, 44)                                                                                                              \
    X(brake, 48)                                                                                                                 \
    X(clutch, 52)                                                                                                                \
    X(steer, 56)                                                                                                                 \
    X(g_lat, 60)                                                                                                                 \
    X(g_long, 64)                                                                                                                \
    X(world_pos, 68)                                                                                                             \
    X(world_vel, 80)                                                                                                             \
    X(ers_store, 92)                                                                                                             \
    X(ers_capacity, 96)                                                                                                          \
    X(ers_deploy, 100)                                                                                                           \
    X(ers_harvest_k, 104)                                                                                                        \
    X(ers_harvest_h, 108)                                                                                                        \
    X(ers_max_deploy, 112)                                                                                                       \
    X(ers_harvested_lap, 116)                                                                                                    \
    X(ers_deployed_lap, 120)                                                                                                     \
    X(ers_harvest_limit_lap, 124)                                                                                                \
    X(ers_mode, 128)                                                                                                             \
    X(lap, 132)                                                                                                                  \
    X(lap_m, 136)                                                                                                                \
    X(total_m, 140)                                                                                                              \
    X(position, 144)                                                                                                             \
    X(result_status, 148)                                                                                                        \
    X(pit_state, 149)                                                                                                            \
    X(tyre_visual, 150)                                                                                                          \
    X(tyre_actual, 151)                                                                                                          \
    X(time_penalty_s, 152)                                                                                                       \
    X(gap_leader_ms, 156)                                                                                                        \
    X(gap_ahead_ms, 160)                                                                                                         \
    X(drive_through, 164)                                                                                                        \
    X(stop_go, 165)                                                                                                              \
    X(warnings, 166)                                                                                                             \
    X(track_limit_warnings, 167)                                                                                                 \
    X(tyre_letter, 168)                                                                                                          \
    X(sector, 169)                                                                                                               \
    X(drs, 170)                                                                                                                  \
    X(pad2, 171)                                                                                                                 \
    X(pit_status, 172)                                                                                                           \
    X(pit_stops, 173)                                                                                                            \
    X(pad3, 174)

#define ADDED                                                                                                                    \
    X(garage, 176)                                                                                                               \
    X(pit_box, 177)                                                                                                              \
    X(pad4, 178)                                                                                                                 \
    X(tyre_damage, 180)                                                                                                          \
    X(tyre_deflation, 184)                                                                                                       \
    X(reserved, 200)

static_assert(sizeof(SwoftyLiveCar) == 208 && alignof(SwoftyLiveCar) == 8);
#define X(f, at) static_assert(offsetof(SwoftyLiveCar, f) == (at), #f);
FROZEN
ADDED
#undef X

int main()
{
    expect(sizeof(SwoftyLiveCar) == 208, "SwoftyLiveCar is 208 bytes, as in every earlier release");
    bool all = true;
#define X(f, at) all = all && offsetof(SwoftyLiveCar, f) == (at);
    FROZEN
#undef X
    expect(all, "every field up to pad3 keeps its offset");
    expect(offsetof(SwoftyLiveCar, garage) == 176 && offsetof(SwoftyLiveCar, pit_box) == 177,
           "garage and pit_box take the first word of the old reserved[7]");
    expect(offsetof(SwoftyLiveCar, tyre_damage) == 180 && offsetof(SwoftyLiveCar, tyre_deflation) == 184,
           "tyre_damage and tyre_deflation take the next five words of the old reserved[7]");
    expect(offsetof(SwoftyLiveCar, reserved) + sizeof(((SwoftyLiveCar *)nullptr)->reserved) == 204,
           "reserved[1] ends where reserved[7] ended");
    expect(SWOFTY_LIVE_HAVE_TYRE_DAMAGE == (1ull << 28) && !(SWOFTY_LIVE_HAVE_TYRE_DAMAGE & SWOFTY_LIVE_HAVE_GARAGE),
           "the tyre damage have bit is a new bit");
    expect(SWOFTY_LIVE_HAVE_GARAGE == (1ull << 27) &&
               !(SWOFTY_LIVE_HAVE_GARAGE & (SWOFTY_LIVE_HAVE_PIT_LANE | SWOFTY_LIVE_HAVE_GAP_AHEAD)),
           "the garage have bit is a new bit");
    SwoftyLiveCar c{};
    expect(!swofty::pitlane::garage(c) && !swofty::pitlane::pit_box(c) && !swofty::pitlane::safety_car(c),
           "without the walk (have bit clear, fields 0) the pit lane reads give nothing");
    c.have = SWOFTY_LIVE_HAVE_GARAGE;
    c.garage = 21;
    c.pit_box = 10;
    expect(swofty::pitlane::garage(c) == 21 && swofty::pitlane::pit_box(c) == 10 && swofty::pitlane::side(c) == 1 &&
               !swofty::pitlane::safety_car(c),
           "garage 21: pit box 10, the slot's second side");
    c.garage = SWOFTY_LIVE_GARAGE_SAFETY_CAR;
    c.pit_box = swofty::pitlane::safety_car_box;
    expect(swofty::pitlane::safety_car(c) && !swofty::pitlane::side(c) && swofty::pitlane::pit_box(c) == 12,
           "the safety car: garage 99, box 12, no side");
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
