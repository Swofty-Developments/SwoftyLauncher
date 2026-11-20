// Host unit test for race control (std lib race_control): the tracker that turns live snapshots into session.flag,
// session.safety_car.deploy, session.red_flag, session.car.status and session.sector moments
// (core/live/race_control.hpp), and the SDK reads in swofty/systems/race_control.hpp against the fake live state.
#include "core/live/race_control.hpp"

#include <swofty/fake_live.hpp>

#include <cstdio>
#include <string>

namespace lv = swofty_core::live;
namespace rcl = swofty_core::live::race_control;
namespace rc = swofty::race_control;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static lv::Snapshot session(unsigned cars = 4)
{
    lv::Snapshot s{};
    s.valid = true;
    SwoftyLiveSession &ses = s.ses;
    ses.version = SWOFTY_LIVE_VERSION;
    ses.size = sizeof ses;
    ses.have = SWOFTY_LIVE_SHAVE_TYPE | SWOFTY_LIVE_SHAVE_CARS | SWOFTY_LIVE_SHAVE_YELLOW | SWOFTY_LIVE_SHAVE_RED_FLAG |
               SWOFTY_LIVE_SHAVE_SAFETY_CAR | SWOFTY_LIVE_SHAVE_ORDER | SWOFTY_LIVE_SHAVE_TRACK_NAME |
               SWOFTY_LIVE_SHAVE_GAME_MODE;
    ses.type = 15;
    ses.generation = 3;
    ses.game_mode = SWOFTY_LIVE_MODE_GRAND_PRIX;
    std::snprintf(ses.track_name, sizeof ses.track_name, "%s", "spa");
    ses.cars = ses.racers = cars;
    ses.classified = cars;
    for (unsigned i = 0; i < cars; i++) {
        ses.order[i] = i;
        s.present[i] = true;
        SwoftyLiveCar &c = s.cars[i];
        c.version = SWOFTY_LIVE_VERSION;
        c.size = sizeof c;
        c.vehicle = i;
        c.have = SWOFTY_LIVE_HAVE_CLASS | SWOFTY_LIVE_HAVE_PROGRESS | SWOFTY_LIVE_HAVE_SECTOR;
        c.result_status = uint8_t(rc::Status::running);
        c.sector = 1;
        c.lap = 7;
        c.position = int32_t(i + 1);
    }
    return s;
}

static void test_first_read_is_silent()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    s.ses.yellow_sectors = 0x2;
    s.ses.safety_car = 1;
    rcl::Moments m = t.step(&s);
    expect(t.primed() && t.sessions() == 1 && m.empty(), "the first read of a session sets the state and fires nothing");
    m = t.step(&s);
    expect(m.empty(), "the same snapshot again fires nothing");
}

static void test_flags()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.ses.yellow_sectors = 0x2;
    rcl::Moments m = t.step(&s);
    expect(m.flags.size() == 1 && m.flags[0].flag == rc::Flag::yellow && m.flags[0].previous == rc::Flag::green &&
               m.flags[0].yellow_sectors == 0x2 && m.flags[0].previous_yellow_sectors == 0 && m.flags[0].lap == 7,
           "a yellow zone in sector 2 fires green to yellow, sectors 0x2, on the leader's lap");
    expect(m.flags[0].size == sizeof(rc::events::Flag) && m.flags[0].payload_version == rc::events::Flag::version,
           "the payload carries its size and version");
    s.ses.yellow_sectors = 0x6;
    m = t.step(&s);
    expect(m.flags.size() == 1 && m.flags[0].flag == rc::Flag::yellow && m.flags[0].previous == rc::Flag::yellow &&
               m.flags[0].yellow_sectors == 0x6 && m.flags[0].previous_yellow_sectors == 0x2,
           "a second yellow sector fires yellow to yellow with both sector sets");
    s.ses.yellow_sectors = 0;
    m = t.step(&s);
    expect(m.flags.size() == 1 && m.flags[0].flag == rc::Flag::green && m.flags[0].previous == rc::Flag::yellow,
           "the zones clearing fires green");
    expect(m.red_flags.empty() && m.safety_cars.empty() && m.statuses.empty() && m.sectors.empty(),
           "a flag change fires nothing else");
}

static void test_safety_car()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.ses.safety_car = 2;
    rcl::Moments m = t.step(&s);
    expect(m.safety_cars.size() == 1 && m.safety_cars[0].deployment == rc::Deployment::virtual_safety_car &&
               m.safety_cars[0].previous == rc::Deployment::none && m.safety_cars[0].lap == 7,
           "the virtual safety car fires");
    s.ses.safety_car = 1;
    m = t.step(&s);
    expect(m.safety_cars.size() == 1 && m.safety_cars[0].deployment == rc::Deployment::safety_car &&
               m.safety_cars[0].previous == rc::Deployment::virtual_safety_car,
           "the virtual safety car becoming the safety car fires");
    s.ses.safety_car = 0;
    m = t.step(&s);
    expect(m.safety_cars.size() == 1 && m.safety_cars[0].deployment == rc::Deployment::none,
           "the safety car coming in fires none");
    expect(m.flags.empty(), "the safety car does not change the flag");
}

static void test_red_flag()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    s.ses.safety_car = 1;
    s.ses.yellow_sectors = 0x1;
    t.step(&s);
    s.ses.red_flag = 1;
    s.ses.safety_car = 0;
    rcl::Moments m = t.step(&s);
    expect(m.red_flags.size() == 1 && m.red_flags[0].state == rc::RedFlag::deployed &&
               m.red_flags[0].previous == rc::RedFlag::none,
           "the red flag fires deployed");
    expect(m.safety_cars.size() == 1 && m.safety_cars[0].deployment == rc::Deployment::none,
           "the safety car reads none under the red flag and fires in");
    expect(m.flags.size() == 1 && m.flags[0].flag == rc::Flag::red && m.flags[0].previous == rc::Flag::yellow,
           "the flag goes yellow to red");
    s.ses.red_flag = 2;
    m = t.step(&s);
    expect(m.red_flags.size() == 1 && m.red_flags[0].state == rc::RedFlag::suspended && m.flags.empty(),
           "suspended fires a red flag moment and keeps the flag red");
    s.ses.red_flag = 0;
    s.ses.yellow_sectors = 0;
    m = t.step(&s);
    expect(m.red_flags.size() == 1 && m.red_flags[0].state == rc::RedFlag::none && m.flags.size() == 1 &&
               m.flags[0].flag == rc::Flag::green && m.flags[0].previous == rc::Flag::red,
           "withdrawing the red flag fires none and the flag goes green");
}

static void test_chequered_and_status()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.ses.chequered = 1;
    s.cars[0].result_status = uint8_t(rc::Status::finished);
    s.cars[0].lap = 8;
    rcl::Moments m = t.step(&s);
    expect(m.flags.size() == 1 && m.flags[0].flag == rc::Flag::chequered && m.flags[0].lap == 8,
           "the leader finishing shows the chequered flag");
    expect(m.statuses.size() == 1 && m.statuses[0].car == 0 && m.statuses[0].status == rc::Status::finished &&
               m.statuses[0].previous == rc::Status::running && m.statuses[0].lap == 8,
           "the leader's status goes running to finished");
    s.ses.yellow_sectors = 0x4;
    m = t.step(&s);
    expect(m.flags.size() == 1 && m.flags[0].flag == rc::Flag::chequered && m.flags[0].yellow_sectors == 0x4,
           "a yellow after the chequered flag keeps it chequered and fires the new sectors");
    s.cars[2].result_status = uint8_t(rc::Status::retired);
    s.cars[3].result_status = uint8_t(rc::Status::disqualified);
    m = t.step(&s);
    expect(m.statuses.size() == 2 && m.statuses[0].car == 2 && m.statuses[0].status == rc::Status::retired &&
               m.statuses[1].car == 3 && m.statuses[1].status == rc::Status::disqualified,
           "a retirement and a disqualification fire in vehicle order");
}

static void test_sectors()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.cars[1].sector = 2;
    s.cars[3].sector = 2;
    rcl::Moments m = t.step(&s);
    expect(m.sectors.size() == 2 && m.sectors[0].car == 1 && m.sectors[0].sector == 2 && m.sectors[0].previous == 1 &&
               m.sectors[0].lap == 7 && m.sectors[1].car == 3,
           "two cars entering sector 2 fire two sector moments");
    s.cars[1].sector = 0;
    m = t.step(&s);
    expect(m.sectors.empty(), "sector 0 (no sector read) fires nothing");
    s.cars[1].sector = 3;
    m = t.step(&s);
    expect(m.sectors.empty(), "the next sector after an unread one sets the state again without a moment");
    s.cars[1].sector = 1;
    s.cars[1].lap = 8;
    m = t.step(&s);
    expect(m.sectors.size() == 1 && m.sectors[0].sector == 1 && m.sectors[0].previous == 3 && m.sectors[0].lap == 8,
           "crossing the line fires sector 1 on the new lap");
}

static void test_fail_closed()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.ses.have &=
        ~(SWOFTY_LIVE_SHAVE_YELLOW | SWOFTY_LIVE_SHAVE_RED_FLAG | SWOFTY_LIVE_SHAVE_ORDER | SWOFTY_LIVE_SHAVE_SAFETY_CAR);
    s.ses.yellow_sectors = 0x1;
    s.ses.safety_car = 0xff;
    rcl::Moments m = t.step(&s);
    expect(m.empty(), "a session read without the flag, safety car and red flag bits fires nothing");
    s.ses.have |= SWOFTY_LIVE_SHAVE_YELLOW | SWOFTY_LIVE_SHAVE_RED_FLAG | SWOFTY_LIVE_SHAVE_ORDER | SWOFTY_LIVE_SHAVE_SAFETY_CAR;
    s.ses.safety_car = 1;
    m = t.step(&s);
    expect(m.empty(), "the bits coming back set the state again without a moment");
    s.cars[2].have &= ~SWOFTY_LIVE_HAVE_CLASS;
    s.cars[2].result_status = 0;
    m = t.step(&s);
    expect(m.statuses.empty(), "a car whose classification is not read fires no status");
    s.cars[2].have |= SWOFTY_LIVE_HAVE_CLASS;
    s.cars[2].result_status = uint8_t(rc::Status::retired);
    m = t.step(&s);
    expect(m.statuses.empty(), "its classification coming back sets the status again without a moment");
    s.present[1] = false;
    t.step(&s);
    s.present[1] = true;
    s.cars[1].sector = 3;
    m = t.step(&s);
    expect(m.sectors.empty() && m.statuses.empty(), "a car that leaves the table and comes back fires nothing");
}

static void test_session_boundaries()
{
    rcl::Tracker t;
    lv::Snapshot s = session();
    t.step(&s);
    s.ses.generation = 4;
    s.ses.yellow_sectors = 0x1;
    s.cars[0].sector = 3;
    rcl::Moments m = t.step(&s);
    expect(m.empty() && t.sessions() == 2, "a new car set generation is a new session: its first read fires nothing");
    s.ses.type = 10;
    s.ses.yellow_sectors = 0;
    m = t.step(&s);
    expect(m.empty() && t.sessions() == 3, "a new session type is a new session");
    std::snprintf(s.ses.track_name, sizeof s.ses.track_name, "%s", "monza");
    s.ses.red_flag = 1;
    m = t.step(&s);
    expect(m.empty() && t.sessions() == 4, "a new track is a new session");
    s.valid = false;
    m = t.step(&s);
    expect(m.empty() && !t.primed(), "a snapshot that is not valid ends the session");
    m = t.step(nullptr);
    expect(m.empty() && !t.primed(), "no snapshot ends the session");
    s.valid = true;
    s.ses.red_flag = 0;
    m = t.step(&s);
    expect(m.empty() && t.primed() && t.sessions() == 5, "the next valid read starts a session again and fires nothing");
}

static void test_sdk_reads()
{
    lv::Snapshot s = session();
    expect(rc::read(s.ses).flag == rc::Flag::green, "zones read and clear: green");
    s.ses.yellow_sectors = 0x1;
    expect(rc::read(s.ses).flag == rc::Flag::yellow && rc::read(s.ses).yellow_sectors == 0x1, "a yellow zone: yellow");
    s.ses.chequered = 1;
    expect(rc::read(s.ses).flag == rc::Flag::chequered, "chequered beats yellow");
    s.ses.red_flag = 1;
    expect(rc::read(s.ses).flag == rc::Flag::red && rc::read(s.ses).red_flag == rc::RedFlag::deployed, "red beats chequered");
    s.ses.have &= ~(SWOFTY_LIVE_SHAVE_RED_FLAG | SWOFTY_LIVE_SHAVE_ORDER);
    expect(rc::read(s.ses).flag == rc::Flag::yellow && rc::read(s.ses).red_flag == rc::RedFlag::unknown,
           "without the red flag and classification bits the zones decide");
    s.ses.have &= ~SWOFTY_LIVE_SHAVE_YELLOW;
    expect(rc::read(s.ses).flag == rc::Flag::unknown && rc::read(s.ses).yellow_sectors == 0,
           "without any bit the flag is unknown");
    s.ses.have &= ~SWOFTY_LIVE_SHAVE_SAFETY_CAR;
    expect(rc::read(s.ses).safety_car == rc::Deployment::unknown, "without the safety car bit the safety car is unknown");
    expect(std::string(rc::name(rc::Flag::chequered)) == "chequered" && std::string(rc::name(rc::Status::retired)) == "retired" &&
               std::string(rc::name(rc::Deployment::virtual_safety_car)) == "virtual safety car" &&
               std::string(rc::name(rc::RedFlag::suspended)) == "suspended",
           "the names");

    expect(!rc::now(), "now() is empty without a live state");
    swofty::fake::Live fake;
    SwoftyLiveSession &ses = fake.session();
    ses.have |= SWOFTY_LIVE_SHAVE_YELLOW | SWOFTY_LIVE_SHAVE_ORDER | SWOFTY_LIVE_SHAVE_SAFETY_CAR;
    ses.yellow_sectors = 0x4;
    ses.safety_car = 2;
    SwoftyLiveCar &lead = fake.add_car(5, 1);
    lead.have |= SWOFTY_LIVE_HAVE_PROGRESS | SWOFTY_LIVE_HAVE_SECTOR;
    lead.lap = 12;
    lead.sector = 3;
    lead.result_status = uint8_t(rc::Status::running);
    fake.add_car(2, 2);
    fake.set_order({5, 2});
    fake.install();
    auto st = rc::now();
    expect(st && st->flag == rc::Flag::yellow && st->yellow_sectors == 0x4 &&
               st->safety_car == rc::Deployment::virtual_safety_car && st->lap == 12 && st->red_flag == rc::RedFlag::unknown,
           "now() reads the fake live session and the leader's lap");
    auto car = swofty::live::car(5);
    expect(car && rc::status(*car) == rc::Status::running && rc::sector(*car) == 3, "status() and sector() read a car");
    fake.uninstall();
}

static void test_points()
{
    expect(rcl::fires("session.flag") && rcl::fires("session.safety_car.deploy") && rcl::fires("session.red_flag") &&
               rcl::fires("session.sector") && rcl::fires("session.car.status"),
           "the five race control points read the live state");
    expect(!rcl::fires("session.lap") && !rcl::fires("session.penalty.issue") && !rcl::fires("flag") && !rcl::fires(""),
           "no other point is gated on the live state");
}

int main()
{
    test_points();
    test_first_read_is_silent();
    test_flags();
    test_safety_car();
    test_red_flag();
    test_chequered_and_status();
    test_sectors();
    test_fail_closed();
    test_session_boundaries();
    test_sdk_reads();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
