// Host unit test for the lap recorder's state machine (native/src/core/lap_rec_logic.hpp), driven by synthetic ticks:
// out-lap, timing, completed laps, and every way a lap is thrown away. Built and run by native/test/run_lap_rec_test.sh.

#include "lap_rec_logic.hpp"

#include <cstdio>

namespace lr = runtime::lap_rec;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Drive {
    lr::Recorder rec;
    double t = 0, x = 0;
    double speed = 60;
    explicit Drive(lr::Config c = {}) : rec(c) {}

    lr::Outcome tick(bool player = true)
    {
        lr::Frame f;
        f.now_s = t;
        f.pos[0] = x;
        f.speed_ms = speed;
        f.player = player;
        uint8_t s[4] = {1, 2, 3, 4};
        lr::Outcome o = rec.step(f, s, 4);
        t += 1.0 / 60;
        x += speed / 60;
        return o;
    }

    void run(double seconds)
    {
        for (int i = 0, n = int(seconds * 60); i < n; i++)
            tick();
    }
};

static void orientation()
{
    float id[3][3] = {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}};
    float q[4];
    lr::quaternion_of(id, q);
    expect(lr::unit_axes(id) && q[0] == 0 && q[1] == 0 && q[2] == 0 && q[3] == 1, "the identity basis is the identity quaternion");
    float yaw90[3][3] = {{0, 0, -1}, {0, 1, 0}, {1, 0, 0}};
    lr::quaternion_of(yaw90, q);
    expect(std::fabs(q[1] - 0.70710678f) < 1e-5f && std::fabs(q[3] - 0.70710678f) < 1e-5f && std::fabs(q[0]) < 1e-5f &&
               std::fabs(q[2]) < 1e-5f,
           "a quarter turn about the up axis is the quaternion (0, sin 45, 0, cos 45)");
    float flip[3][3] = {{1, 0, 0}, {0, -1, 0}, {0, 0, -1}};
    lr::quaternion_of(flip, q);
    expect(std::fabs(q[0] - 1) < 1e-5f && std::fabs(q[3]) < 1e-5f, "a half turn about x takes the largest-diagonal branch");
    float flip_y[3][3] = {{-1, 0, 0}, {0, 1, 0}, {0, 0, -1}};
    lr::quaternion_of(flip_y, q);
    expect(std::fabs(q[1] - 1) < 1e-5f, "a half turn about y");
    float flip_z[3][3] = {{-1, 0, 0}, {0, -1, 0}, {0, 0, 1}};
    lr::quaternion_of(flip_z, q);
    expect(std::fabs(q[2] - 1) < 1e-5f, "a half turn about z");
    float skew[3][3] = {{1, 0, 0}, {1, 0, 0}, {0, 0, 1}};
    float zero[3][3] = {};
    float big[3][3] = {{2, 0, 0}, {0, 1, 0}, {0, 0, 1}};
    expect(!lr::unit_axes(skew) && !lr::unit_axes(zero) && !lr::unit_axes(big),
           "axes that are not orthonormal are refused");
}

static void plans()
{
    lr::Avail all;
    all.pos = all.vel = all.pedals = all.steer = all.gear = all.rpm = all.speed = all.orient = all.spin = all.g = all.drs =
        all.surface = all.slip = true;
    lr::Plan full = lr::plan_of(all);
    expect(full.layout.columns.size() == size_t(lr::Col::count) && full.missing.empty(),
           "with everything resolved every column is in the table");
    expect(full.layout.index("t") == 0 && full.layout.index("px") == 2 && full.layout.index("gear") >= 0,
           "the required fields are present by their contract names");
    bool required = true;
    for (const char *n : {"t", "px", "py", "pz", "vx", "vy", "vz", "throttle", "brake", "steer", "gear", "rpm", "speed"})
        required = required && full.layout.index(n) >= 0;
    expect(required, "t, px..pz, vx..vz, throttle, brake, steer, gear, rpm and speed are all there");
    expect(full.layout.columns[size_t(full.layout.index("gear"))].type == runtime::lap_file::Type::i8,
           "gear is an i8");
    bool order = true;
    for (size_t i = 0; i < full.ids.size(); i++)
        order = order && size_t(full.ids[i]) == i;
    expect(order, "the column ids follow the table order");

    lr::Avail some;
    some.pos = some.vel = some.speed = true;
    lr::Plan part = lr::plan_of(some);
    expect(part.layout.index("qx") == -1 && part.layout.index("rpm") == -1 && part.layout.index("vx") >= 0,
           "a field that did not resolve is left out of the table");
    expect(part.missing.find("rpm") != std::string::npos && part.missing.find("ws_fl") != std::string::npos &&
               part.missing.find("px") == std::string::npos,
           "the plan names what is missing");
    expect(part.ids.size() == part.layout.columns.size(), "ids and columns stay the same length");
    lr::Plan none = lr::plan_of(lr::Avail{});
    expect(none.layout.columns.size() == 1 && none.layout.columns[0].name == "t", "the clock column always exists");

    lr::Gate g = lr::gate_of("{\"tt_boards\": true, \"tt_nonce\": \"abc123\", \"native\": []}");
    expect(g.on && g.nonce == "abc123", "session.json with tt_boards on opens the gate and carries the nonce");
    g = lr::gate_of("{\"tt_boards\": true}");
    expect(g.on && g.nonce.empty(), "the nonce is optional");
    expect(!lr::gate_of("{\"tt_boards\": false, \"tt_nonce\": \"x\"}").on, "tt_boards off keeps the gate shut");
    expect(!lr::gate_of("{}").on && !lr::gate_of("").on && !lr::gate_of("not json").on && !lr::gate_of("[true]").on,
           "a missing key, an empty file or garbage keeps the gate shut");
    expect(!lr::gate_of("{\"tt_boards\": \"yes\"}").on, "only a JSON true opens the gate");

    expect(std::string(lr::weather_of(true, 0.0)) == "dry" && std::string(lr::weather_of(true, 0.8)) == "wet" &&
               std::string(lr::weather_of(false, 0.8)).empty(),
           "weather comes from the track wetness and stays unknown when unread");
    expect(lr::file_name(2026, 11, 10, 4, 10, 11, 3) == "20261110T041011Z-3.swlap", "file names are a UTC stamp and the lap number");
}

int main()
{
    orientation();
    plans();
    {
        Drive d;
        expect(d.rec.state() == lr::State::idle, "a recorder starts idle");
        d.tick();
        expect(d.rec.samples() == 0, "an idle recorder keeps nothing");
        expect(d.rec.newlap(d.t, true).kind == lr::Outcome::none, "a line crossing while idle does nothing");
        d.rec.begin_session();
        expect(d.rec.state() == lr::State::outlap, "a session starts on the out-lap");
        d.run(5);
        expect(d.rec.samples() == 0, "the out-lap is not recorded");
        expect(d.rec.newlap(d.t, true).kind == lr::Outcome::started && d.rec.state() == lr::State::timing,
               "the first crossing starts timing");
        d.run(60);
        expect(d.rec.samples() == 3600, "a minute at 60 Hz is 3600 samples");
        auto o = d.rec.newlap(d.t, true);
        expect(o.kind == lr::Outcome::completed && o.lap.count == 3600 && o.lap.samples.size() == 3600 * 4,
               "the next crossing completes the lap with its samples");
        expect(o.lap.index == 1 && o.lap.ended_s - o.lap.started_s > 59.9 && o.lap.ended_s - o.lap.started_s < 60.1,
               "the lap carries its index and times");
        expect(o.lap.path_m > 3500 && o.lap.path_m < 3700, "the path length is the distance driven");
        expect(d.rec.state() == lr::State::timing && d.rec.samples() == 0, "the next lap starts at the crossing");
        d.run(60);
        expect(d.rec.newlap(d.t, true).lap.index == 2, "laps are numbered in order");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(30);
        expect(d.rec.newlap(d.t, false).kind == lr::Outcome::none && d.rec.samples() == 1800,
               "an AI car's crossing is ignored");
        d.tick(false);
        expect(d.rec.samples() == 1800, "an AI car's tick is ignored");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(10);
        auto o = d.rec.newlap(d.t, true);
        expect(o.kind == lr::Outcome::discarded && o.why == lr::Discard::too_short,
               "a lap under the minimum time is discarded (a backward crossing)");
        expect(d.rec.state() == lr::State::timing, "timing restarts after a discarded crossing");
    }
    {
        Drive d;
        d.speed = 1.5;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(40);
        expect(d.rec.newlap(d.t, true).why == lr::Discard::backwards, "a lap that covered no ground is discarded");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(20);
        d.x += 5000;
        auto o = d.tick();
        expect(o.kind == lr::Outcome::discarded && o.why == lr::Discard::teleport, "a jump in position discards the lap");
        expect(d.rec.state() == lr::State::outlap && d.rec.samples() == 0, "the recorder waits for the next crossing");
        d.run(30);
        expect(d.rec.samples() == 0, "nothing is recorded until the line is crossed");
        d.rec.newlap(d.t, true);
        d.run(60);
        expect(d.rec.newlap(d.t, true).kind == lr::Outcome::completed, "the lap after a reset is recorded");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(20);
        d.speed = 0;
        auto o = d.tick();
        expect(o.kind == lr::Outcome::discarded && o.why == lr::Discard::reset, "stopping dead in one tick is a reset");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(20);
        d.t += 6;
        d.x += 6 * 60;
        expect(d.tick().why == lr::Discard::teleport, "a stall longer than the gap limit discards the lap");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(20);
        d.t += 0.2;
        d.x += 0.2 * 60;
        expect(d.tick().kind == lr::Outcome::none, "a slow frame at speed is not a teleport");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(20);
        auto o = d.rec.reset(lr::Discard::reset);
        expect(o.kind == lr::Outcome::discarded && d.rec.state() == lr::State::outlap, "an explicit reset discards");
        d.run(2);
        o = d.rec.reset(lr::Discard::reset);
        expect(o.kind == lr::Outcome::none, "a reset on the out-lap discards nothing");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(20);
        auto o = d.rec.end_session();
        expect(o.kind == lr::Outcome::discarded && o.why == lr::Discard::left_session && d.rec.state() == lr::State::idle,
               "leaving the session discards the lap in progress");
        expect(d.rec.end_session().kind == lr::Outcome::none, "ending twice is harmless");
    }
    {
        lr::Config c;
        c.max_samples = 100;
        Drive d(c);
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        lr::Outcome last;
        for (int i = 0; i < 200 && last.kind == lr::Outcome::none; i++)
            last = d.tick();
        expect(last.why == lr::Discard::capped && d.rec.samples() == 0, "a lap past the sample cap is discarded");
    }
    {
        lr::Config c;
        c.min_samples = 500;
        Drive d(c);
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        for (int i = 0; i < 40; i++) {
            d.t += 1.0;
            d.x += 60;
            d.tick();
        }
        d.t += 1;
        auto o = d.rec.newlap(d.t, true);
        expect(o.why == lr::Discard::too_few_samples, "a lap with too few samples is discarded");
    }
    {
        Drive d;
        d.rec.begin_session();
        d.run(2);
        d.rec.newlap(d.t, true);
        d.run(60);
        d.rec.begin_session();
        expect(d.rec.state() == lr::State::outlap && d.rec.samples() == 0 && d.rec.laps_written() == 0,
               "a new session starts clean");
    }

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
