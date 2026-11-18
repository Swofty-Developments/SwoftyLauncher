// Host unit test for tyre damage (std lib tyre_damage): the tracker that turns live snapshots into car.tyre.damage
// moments (core/live/tyre_damage.hpp), and the SDK reads in swofty/systems/tyre_damage.hpp.
#include "core/live/tyre_damage.hpp"

#include <cstdio>
#include <string>

namespace lv = swofty_core::live;
namespace tdl = swofty_core::live::tyre_damage;
namespace td = swofty::tyre_damage;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static lv::Snapshot session(unsigned cars = 3)
{
    lv::Snapshot s{};
    s.valid = true;
    SwoftyLiveSession &ses = s.ses;
    ses.version = SWOFTY_LIVE_VERSION;
    ses.size = sizeof ses;
    ses.have = SWOFTY_LIVE_SHAVE_TYPE | SWOFTY_LIVE_SHAVE_CARS | SWOFTY_LIVE_SHAVE_TRACK_NAME;
    ses.type = 15;
    ses.generation = 3;
    std::snprintf(ses.track_name, sizeof ses.track_name, "%s", "spa");
    ses.cars = ses.racers = cars;
    for (unsigned i = 0; i < cars; i++) {
        s.present[i] = true;
        SwoftyLiveCar &c = s.cars[i];
        c.version = SWOFTY_LIVE_VERSION;
        c.size = sizeof c;
        c.vehicle = i;
        c.have = SWOFTY_LIVE_HAVE_PROGRESS | SWOFTY_LIVE_HAVE_TYRE_DAMAGE;
        c.lap = 12;
    }
    return s;
}

static void test_reads()
{
    SwoftyLiveCar c{};
    expect(!td::state(c, td::Wheel::front_left) && !td::deflation(c, td::Wheel::front_left) && !td::damaged(c),
           "without the read (have bit clear) the tyre damage reads give nothing");
    c.have = SWOFTY_LIVE_HAVE_TYRE_DAMAGE;
    c.tyre_damage[2] = SWOFTY_LIVE_TYRE_PUNCTURED;
    c.tyre_deflation[2] = 1.4f;
    c.tyre_damage[1] = SWOFTY_LIVE_TYRE_BURST;
    c.tyre_damage[0] = 1;
    expect(td::state(c, td::Wheel::front_left) == td::State::punctured && td::deflation(c, td::Wheel::front_left) == 1.4f &&
               td::state(c, td::Wheel::rear_right) == td::State::burst && td::state(c, td::Wheel::rear_left) == td::State::none &&
               td::damaged(c) == 0x6,
           "a punctured FL and a burst RR read as such; a value that is neither reads none");
    expect(!td::state(c, td::Wheel(4)), "a wheel past the fourth reads nothing");
    expect(std::string(td::name(td::Wheel::front_right)) == "FR" && std::string(td::name(td::State::burst)) == "burst",
           "wheel and state names");
}

static void test_first_read_is_silent()
{
    tdl::Tracker t;
    lv::Snapshot s = session();
    s.cars[1].tyre_damage[3] = SWOFTY_LIVE_TYRE_PUNCTURED;
    expect(t.step(&s).empty() && t.primed(), "the first read of a session sets the state and fires nothing");
    expect(t.step(&s).empty(), "an unchanged read fires nothing");
}

static void test_puncture_then_burst_then_change()
{
    tdl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.cars[2].tyre_damage[0] = SWOFTY_LIVE_TYRE_PUNCTURED;
    s.cars[2].tyre_deflation[0] = 1.0f;
    auto m = t.step(&s);
    expect(m.size() == 1 && m[0].car == 2 && m[0].wheel == td::Wheel::rear_left && m[0].state == td::State::punctured &&
               m[0].previous == td::State::none && m[0].deflation == 1.0f && m[0].lap == 12 &&
               m[0].size == sizeof(td::events::Damage) && m[0].payload_version == td::events::Damage::version,
           "a puncture fires once, with the wheel, the deflation and the car's lap");
    s.cars[2].tyre_damage[0] = SWOFTY_LIVE_TYRE_BURST;
    s.cars[2].tyre_deflation[0] = 2.0f;
    m = t.step(&s);
    expect(m.size() == 1 && m[0].state == td::State::burst && m[0].previous == td::State::punctured,
           "the burst fires from punctured");
    s.cars[2].tyre_damage[0] = 0;
    s.cars[2].tyre_deflation[0] = 0;
    s.cars[0].tyre_damage[3] = SWOFTY_LIVE_TYRE_PUNCTURED;
    m = t.step(&s);
    expect(m.size() == 2 && m[0].car == 0 && m[0].wheel == td::Wheel::front_right && m[1].car == 2 &&
               m[1].state == td::State::none && m[1].previous == td::State::burst,
           "a tyre change back to none fires too, car by car in vehicle order");
}

static void test_forget_and_new_session()
{
    tdl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.cars[1].have &= ~SWOFTY_LIVE_HAVE_TYRE_DAMAGE;
    s.cars[1].tyre_damage[0] = SWOFTY_LIVE_TYRE_PUNCTURED;
    expect(t.step(&s).empty(), "a car whose read is off fires nothing");
    s.cars[1].have |= SWOFTY_LIVE_HAVE_TYRE_DAMAGE;
    expect(t.step(&s).empty(), "the car's read coming back sets its state again without a moment");
    uint64_t before = t.sessions();
    s.ses.generation = 4;
    s.cars[0].tyre_damage[1] = SWOFTY_LIVE_TYRE_PUNCTURED;
    expect(t.step(&s).empty() && t.sessions() == before + 1, "a new session starts silent");
    expect(t.step(nullptr).empty() && !t.primed(), "no snapshot resets the tracker");
}

int main()
{
    test_reads();
    test_first_read_is_silent();
    test_puncture_then_burst_then_change();
    test_forget_and_new_session();
    expect(tdl::fires("car.tyre.damage") && !tdl::fires("session.flag"), "the point the tracker fires");
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
