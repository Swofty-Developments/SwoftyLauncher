// Host unit test for the live state's UDP source (core/live/live_udp.hpp): the session clock, lap count, pit window,
// per-car DRS, pit limiter, FIA flag, driver status, pit timing and identity decoded from the game's UDP packets and
// merged into a snapshot without overwriting the memory read, the race control messages it queues, the broadcast
// moments the race control tracker fires from them, and the SDK reads in swofty/live.hpp.
#include "core/live/live_hud.hpp"
#include "core/live/live_udp.hpp"
#include "core/live/race_control.hpp"

#include <swofty/fake_live.hpp>

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

namespace lv = swofty_core::live;
namespace udp = swofty_core::live::udp;
namespace rcl = swofty_core::live::race_control;
namespace rc = swofty::race_control;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

using Bytes = std::vector<uint8_t>;

static void put16(Bytes &b, size_t at, unsigned v)
{
    b[at] = v & 255;
    b[at + 1] = v >> 8 & 255;
}

static void put32(Bytes &b, size_t at, uint32_t v)
{
    for (int i = 0; i < 4; i++)
        b[at + i] = v >> (8 * i) & 255;
}

static void putf(Bytes &b, size_t at, float f)
{
    uint32_t v;
    std::memcpy(&v, &f, 4);
    put32(b, at, v);
}

static Bytes packet(int format, int id, size_t size, uint64_t uid = 77)
{
    Bytes b(size, 0);
    put16(b, 0, format);
    b[6] = id;
    put32(b, 7, uint32_t(uid));
    put32(b, 11, uint32_t(uid >> 32));
    return b;
}

static Bytes event(const char *code, std::vector<uint8_t> details = {}, int format = 2025)
{
    Bytes b = packet(format, 3, 45);
    std::memcpy(&b[29], code, 4);
    for (size_t i = 0; i < details.size(); i++)
        b[33 + i] = details[i];
    return b;
}

static bool feed(udp::Source &u, const Bytes &b, uint64_t now = 1000) { return u.feed(b.data(), b.size(), now); }

static lv::Snapshot snapshot(unsigned cars = 4)
{
    lv::Snapshot s{};
    s.valid = true;
    s.ses.have = SWOFTY_LIVE_SHAVE_TYPE | SWOFTY_LIVE_SHAVE_ORDER | SWOFTY_LIVE_SHAVE_SAFETY_CAR;
    s.ses.type = 15;
    s.ses.classified = cars;
    for (unsigned i = 0; i < cars; i++) {
        s.ses.order[i] = i;
        s.present[i] = true;
        s.cars[i].vehicle = i;
        s.cars[i].have = SWOFTY_LIVE_HAVE_PROGRESS | SWOFTY_LIVE_HAVE_CLASS;
        s.cars[i].lap = 6;
        s.details[i].have = s.cars[i].have;
    }
    return s;
}

static void merge(const udp::Source &u, lv::Snapshot &s, uint64_t now = 1000)
{
    u.merge(s.ses, s.cars, s.details, s.present, now);
}

static void test_session()
{
    udp::Source u;
    Bytes p = packet(2025, 1, 753);
    p[32] = 57;
    put16(p, 38, 3125);
    put16(p, 40, 3600);
    p[153] = 0;
    p[682] = 18;
    p[683] = 24;
    p[684] = 9;
    expect(feed(u, p), "a 2025 Session packet is read");
    lv::Snapshot s = snapshot();
    merge(u, s);
    expect(swofty::live::total_laps(s.ses) == 57u && swofty::live::time_left_s(s.ses) == 3125.f && s.ses.duration_s == 3600.f,
           "total laps 57, 3125 s left of 3600");
    auto w = swofty::live::pit_window(s.ses);
    expect(w && w->ideal_lap == 18 && w->latest_lap == 24 && w->rejoin_position == 9, "the pit window: laps 18 to 24, rejoin P9");

    lv::Snapshot late = snapshot();
    merge(u, late, 1000 + udp::STALE_MS + 1);
    expect(!(late.ses.have & (SWOFTY_LIVE_SHAVE_TIME_LEFT | SWOFTY_LIVE_SHAVE_TOTAL_LAPS)),
           "a Session packet older than five seconds leaves the bits clear");

    p[153] = 3;
    feed(u, p);
    lv::Snapshot f = snapshot();
    merge(u, f);
    expect(f.ses.safety_car == 3, "safety car status 3 sets the formation lap while memory reads none");
    f = snapshot();
    f.ses.safety_car = 1;
    merge(u, f);
    expect(f.ses.safety_car == 1, "a deployment memory reads is kept");

    Bytes p24 = packet(2024, 1, 753);
    p24[32] = 5;
    expect(feed(u, p24) && !feed(u, packet(2026, 1, 900)), "2024 Session is read, the uncatalogued 2026 one is not");
}

static void test_cars()
{
    udp::Source u;
    Bytes lap = packet(2025, 2, 1285);
    uint8_t *e = &lap[29 + 2 * 57];
    e[32 + 12] = 2;
    e[32 + 14] = 1;
    lap[29 + 2 * 57 + 47] = 0x10, lap[29 + 2 * 57 + 48] = 0x27;
    lap[29 + 2 * 57 + 49] = 0xd0, lap[29 + 2 * 57 + 50] = 0x07;
    feed(u, lap);
    Bytes st = packet(2025, 7, 1239);
    uint8_t *c = &st[29 + 2 * 55];
    c[4] = 1;
    c[22] = 1;
    c[23] = 120;
    c[28] = 2;
    feed(u, st);
    Bytes who = packet(2025, 4, 1284);
    who[29] = 4;
    uint8_t *w = &who[30 + 2 * 57];
    w[1] = 9;
    w[3] = 2;
    w[5] = 81;
    w[44] = 1;
    w[45] = 0xff, w[46] = 0x80, w[47] = 0x00;
    feed(u, who);

    lv::Snapshot s = snapshot();
    merge(u, s);
    const SwoftyLiveCarDetail &d = s.details[2];
    expect(swofty::live::driver_status(d) == swofty::live::DriverStatus::in_lap, "Lap Data driverStatus 2: in lap");
    expect(d.pit_lane_timer_active == 1 && d.pit_lane_ms == 10000 && d.pit_stop_ms == 2000,
           "pit lane timer on, 10.000 s in the lane, 2.000 s in the box");
    expect(swofty::live::drs_allowed(d) == true && d.drs_distance_m == 120.f && swofty::live::pit_limiter(d) == true &&
               swofty::live::fia_flag(d) == int8_t(2),
           "Car Status: DRS allowed 120 m out, limiter on, blue flag");
    expect(swofty::live::race_number(d) == uint8_t(81) && d.team_id == 2 && d.driver_id == 9 &&
               swofty::live::team_colour(d) == 0xff8000u,
           "Participants: number 81, team 2, driver 9, colour ff8000");
    expect((s.cars[2].have & SWOFTY_LIVE_HAVE_PIT_TIMING) && (s.cars[2].have & SWOFTY_LIVE_HAVE_IDENTITY),
           "the car's have bits match the detail's");

    lv::Snapshot m = snapshot();
    m.details[2].have |= SWOFTY_LIVE_HAVE_DRIVER_STATUS;
    m.details[2].driver_status = 4;
    merge(u, m);
    expect(m.details[2].driver_status == 4, "a value memory filled is not overwritten");

    lv::Snapshot absent = snapshot(2);
    merge(u, absent);
    expect(!absent.present[2] && !(absent.details[2].have & SWOFTY_LIVE_HAVE_DRIVER_STATUS),
           "a car not in the snapshot stays empty");

    udp::Source v;
    Bytes st26 = packet(2026, 7, 1445);
    st26[29 + 22] = 1;
    st26[29 + 4] = 1;
    feed(v, st26);
    lv::Snapshot n = snapshot();
    merge(v, n);
    expect(swofty::live::drs_allowed(n.details[0]) == true && !swofty::live::pit_limiter(n.details[0]),
           "2026 Car Status gives DRS allowed but not the limiter");

    Bytes lap24 = packet(2024, 2, 1131);
    lap24[29 + 30 + 12] = 3;
    feed(v, lap24);
    n = snapshot();
    merge(v, n);
    expect(swofty::live::driver_status(n.details[0]) == swofty::live::DriverStatus::out_lap, "2024 Lap Data: out lap");
}

static void test_sector_bests()
{
    udp::Source u;
    Bytes h = packet(2025, 11, 1460);
    h[29] = 1;
    h[30] = 3;
    h[33] = 2, h[34] = 3, h[35] = 0;
    uint8_t *lap2 = &h[36 + 1 * 14];
    lap2[4] = 0x30, lap2[5] = 0x75;
    uint8_t *lap3 = &h[36 + 2 * 14];
    lap3[7] = 0xa8, lap3[8] = 0x61, lap3[9] = 1;
    expect(feed(u, h), "a 2025 Session History packet is read");
    lv::Snapshot s = snapshot();
    merge(u, s);
    auto b = swofty::live::sector_bests(s.details[1]);
    expect(b && (*b)[0] == 30000 && (*b)[1] == 85000 && (*b)[2] == 0,
           "vehicle 1: best sector 1 30.000 on lap 2, sector 2 1:25.000 on lap 3, no sector 3 yet");
    expect(!swofty::live::sector_bests(s.details[0]), "a car with no history packet has no bests");
    h[35] = 9;
    feed(u, h);
    s = snapshot();
    merge(u, s);
    expect(swofty::live::sector_bests(s.details[1]).value()[2] == 0, "a best lap number past numLaps reads as none");
}

static void test_classification()
{
    udp::Source u;
    Bytes fc = packet(2025, 8, 1042);
    fc[29] = 2;
    uint8_t *a = &fc[30];
    a[0] = 2, a[1] = 5, a[2] = 1, a[3] = 18, a[4] = 1, a[5] = 3, a[6] = 0;
    put32(fc, 30 + 7, 91500);
    double race = 512.25;
    uint64_t bits;
    std::memcpy(&bits, &race, 8);
    put32(fc, 30 + 11, uint32_t(bits));
    put32(fc, 30 + 15, uint32_t(bits >> 32));
    fc[30 + 19] = 5;
    uint8_t *b = &fc[30 + 46];
    b[0] = 1, b[1] = 5, b[2] = 3, b[3] = 25, b[5] = 3;
    expect(feed(u, fc), "a 2025 Final Classification packet is read");
    u.end();
    udp::Moments m = u.take();
    expect(m.classifications.size() == 1 && m.classifications[0].cars == 2, "it survives the end of the live session");
    const rc::events::ClassifiedCar &c = m.classifications[0].entries[0];
    expect(c.car == 0 && c.position == 2 && c.laps == 5 && c.grid == 1 && c.points == 18 && c.pit_stops == 1 && c.status == 3 &&
               c.best_lap_ms == 91500 && c.race_ms == 512250 && c.penalties_s == 5,
           "vehicle 0: P2 from P1, 5 laps, 18 points, one stop, best 1:31.500, 8:32.250, 5 s of penalties");
    expect(m.classifications[0].entries[1].position == 1 && m.classifications[0].entries[1].points == 25, "vehicle 1 won");
}

static void test_messages()
{
    udp::Source u;
    feed(u, event("PENA", {4, 17, 3, 255, 5, 12, 0}));
    feed(u, event("PENA", {5, 7, 1, 2, 0, 13, 0}));
    feed(u, event("SGSV", {3, 0, 0, 0x20, 0x41}));
    feed(u, event("DTSV", {1}));
    feed(u, event("PENA", {4, 17, 40, 255, 5, 12, 0}));
    udp::Moments m = u.take();
    expect(m.penalties.size() == 2 && m.penalties[0].car == 3 && m.penalties[0].penalty_type == 4 &&
               m.penalties[0].infringement == 17 && m.penalties[0].seconds == 5 && m.penalties[0].lap == 12 &&
               m.penalties[0].other_car == SWOFTY_LIVE_NO_CAR,
           "PENA: vehicle 3, a 5 s time penalty for pit lane speeding on lap 12, no other car");
    expect(m.penalties[1].other_car == 2 && std::string(rc::penalty_name(m.penalties[1].penalty_type)) == "warning" &&
               std::string(rc::infringement_name(m.penalties[1].infringement)) == "corner cutting gained time",
           "a warning for corner cutting names the other car and reads its names");
    expect(m.penalties[0].size == sizeof(rc::events::Penalty) && m.penalties[0].payload_version == 1,
           "the payload carries its size and version");
    expect(m.served.size() == 2 && m.served[0].kind == rc::Served::stop_go && m.served[0].stop_s == 10.f &&
               m.served[1].kind == rc::Served::drive_through && m.served[1].car == 1,
           "SGSV with a 10 s stop, then DTSV");
    expect(u.take().empty(), "take empties the queue");

    feed(u, event("PENA", {4, 17, 3, 255, 5, 12, 0}), 2000);
    Bytes other = event("DRSE");
    put32(other, 7, 99);
    feed(u, other);
    expect(u.take().empty(), "a new session uid drops what the old session queued");
}

static lv::Snapshot step_with(udp::Source &u, rcl::Tracker &t, rcl::Moments &out, uint64_t now = 1000)
{
    lv::Snapshot s = snapshot();
    merge(u, s, now);
    out = t.step(&s);
    return s;
}

static void test_broadcast_moments()
{
    udp::Source u;
    rcl::Tracker t;
    rcl::Moments m;
    step_with(u, t, m);
    expect(m.empty(), "the first read fires nothing");

    feed(u, event("STLG", {1}));
    step_with(u, t, m);
    expect(m.lights.size() == 1 && m.lights[0].lights == 1 && !m.lights[0].out, "the first start light fires");
    for (uint8_t n = 2; n <= 5; n++)
        feed(u, event("STLG", {n}));
    step_with(u, t, m);
    expect(m.lights.size() == 1 && m.lights[0].lights == 5, "lights 2 to 5 between two reads fire the last one");
    feed(u, event("LGOT"));
    step_with(u, t, m);
    expect(m.lights.size() == 1 && m.lights[0].out && m.lights[0].lights == 0, "lights out");

    feed(u, event("DRSE"));
    step_with(u, t, m);
    expect(m.drs.size() == 1 && m.drs[0].enabled == 1 && m.drs[0].lap == 6, "DRS enabled on the leader's lap");
    feed(u, event("DRSD", {1}));
    step_with(u, t, m);
    expect(m.drs.size() == 1 && m.drs[0].enabled == 0 && m.drs[0].reason == 1, "DRS disabled for the safety car");
    feed(u, event("DRSD", {1}));
    step_with(u, t, m);
    expect(m.drs.empty(), "the same DRS state again fires nothing");

    feed(u, event("SCAR", {1, 0}));
    lv::Snapshot s = snapshot();
    s.ses.safety_car = 1;
    merge(u, s);
    m = t.step(&s);
    expect(m.phases.size() == 1 && m.phases[0].phase == SWOFTY_LIVE_SC_DEPLOYED && m.phases[0].previous == 0xff &&
               m.phases[0].deployment == rc::Deployment::safety_car,
           "SCAR deployed fires the phase with the deployment memory reads");
    feed(u, event("SCAR", {1, 1}));
    s = snapshot();
    s.ses.safety_car = 1;
    merge(u, s);
    m = t.step(&s);
    expect(m.phases.size() == 1 && m.phases[0].phase == SWOFTY_LIVE_SC_RETURNING &&
               std::string(rc::phase_name(m.phases[0].phase)) == "returning",
           "safety car in this lap");

    Bytes ft = event("FTLP", {2});
    putf(ft, 34, 91.234f);
    feed(u, ft);
    step_with(u, t, m);
    expect(m.fastest.size() == 1 && m.fastest[0].car == 2 && m.fastest[0].lap_ms == 91234 &&
               m.fastest[0].previous_car == SWOFTY_LIVE_NO_CAR && m.fastest[0].lap == 6,
           "FTLP: vehicle 2 sets 1:31.234 on lap 6");
    step_with(u, t, m);
    expect(m.fastest.empty(), "the same fastest lap again fires nothing");
}

static void test_fastest_from_memory()
{
    udp::Source u;
    Bytes ft = event("FTLP", {3});
    putf(ft, 34, 80.f);
    feed(u, ft);
    lv::Snapshot s = snapshot();
    for (unsigned i = 0; i < 4; i++) {
        s.cars[i].have |= SWOFTY_LIVE_HAVE_LAP_TIMES;
        s.details[i].have |= SWOFTY_LIVE_HAVE_LAP_TIMES;
    }
    s.details[0].best_lap_ms = 92000;
    s.details[1].best_lap_ms = 90500;
    merge(u, s);
    auto f = swofty::live::fastest_lap(s.ses);
    expect(f && f->vehicle == 1 && f->lap_ms == 90500, "with lap times in memory the fastest lap is memory's best lap");
    lv::Snapshot n = snapshot();
    merge(u, n);
    f = swofty::live::fastest_lap(n.ses);
    expect(f && f->vehicle == 3 && f->lap_ms == 80000, "without them it is FTLP's");
}

static void test_points()
{
    expect(rcl::fires("session.penalty") && rcl::fires("session.penalty.served") && rcl::fires("session.drs") &&
               rcl::fires("session.start_lights") && rcl::fires("session.safety_car.phase") &&
               rcl::fires("session.fastest_lap") && rcl::fires("session.classification"),
           "the broadcast points read the live state");
}

static void test_pause_clock()
{
    udp::Clock c;
    expect(c.step(1000, false) == 1000 && c.step(2000, false) == 2000, "the clock runs with the wall clock");
    c.step(2016, true);
    expect(c.step(62016, true) == 2000 && c.at(62016) == 2000, "a pause stops it: a minute of pause does not age the packets");
    expect(c.step(63016, false) == 3000, "it runs again from the resume");
}

static void test_hud()
{
    namespace fv = frontend::view;
    namespace hud = swofty_core::live::hud;
    SwoftyLiveSession s{};
    auto at = [&](bool seen, uint32_t flags, uint32_t pause) {
        hud::apply(s, hud::from_view(seen, flags, pause));
        return swofty::live::hud_visible(s);
    };
    expect(!at(false, fv::MENU, 0) && !(s.have & SWOFTY_LIVE_SHAVE_HUD), "no flow state seen: the bit stays clear");
    expect(at(true, fv::MENU, 0) == false && swofty::live::hud_hidden_by(s) == SWOFTY_LIVE_HUD_MENU,
           "the track intro and the grid (no driving view) hide it");
    expect(at(true, fv::CUTSCENE, 0) == false && swofty::live::hud_hidden_by(s) == SWOFTY_LIVE_HUD_CUTSCENE,
           "a cutscene hides it");
    expect(at(true, 0, 0) == true && swofty::live::hud_hidden_by(s) == 0, "driving shows it");
    expect(at(true, 0, 1u << 3) == false && swofty::live::hud_hidden_by(s) == SWOFTY_LIVE_HUD_PAUSED, "a pause reason hides it");
    expect(at(true, fv::REPLAY | fv::PHOTO, 0) == false &&
               swofty::live::hud_hidden_by(s) == (SWOFTY_LIVE_HUD_REPLAY | SWOFTY_LIVE_HUD_PHOTO),
           "replay and photo mode hide it");
    expect(at(true, fv::SPECTATING, 0) == true, "spectating shows the spectator HUD");
    expect(!at(false, 0, 0) && s.hud_visible == 0 && s.hud_hidden_by == 0, "losing the flow clears the fields");
}

static void test_sdk_fake()
{
    swofty::fake::Live fake;
    SwoftyLiveSession &ses = fake.session();
    ses.have |= SWOFTY_LIVE_SHAVE_TOTAL_LAPS | SWOFTY_LIVE_SHAVE_TIME_LEFT;
    ses.total_laps = 44;
    ses.time_left_s = 61.5f;
    fake.install();
    auto s = swofty::live::session();
    expect(s && swofty::live::total_laps(*s) == 44u && swofty::live::time_left_s(*s) == 61.5f && !swofty::live::fastest_lap(*s) &&
               !swofty::live::drs_enabled(*s),
           "the fake live session gives total laps and time left, and nothing it does not set");
    fake.uninstall();
}

int main()
{
    test_session();
    test_cars();
    test_sector_bests();
    test_classification();
    test_messages();
    test_broadcast_moments();
    test_fastest_from_memory();
    test_points();
    test_hud();
    test_pause_clock();
    test_sdk_fake();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
