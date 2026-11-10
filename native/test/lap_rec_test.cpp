// Host unit test for the lap recorder's state machine (native/src/core/lap_rec_logic.hpp), driven by synthetic ticks:
// out-lap, timing, completed laps, and every way a lap is thrown away. Built and run by native/test/run_lap_rec_test.sh.

#include "lap_game_logic.hpp"
#include "lap_rec_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>
#include <string>

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
    expect(g.on && g.nonce == "abc123", "tt.json with tt_boards on opens the gate and carries the nonce");
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

struct Fake {
    std::map<uint64_t, uint8_t> bytes;
    template <class T> void put(uint64_t at, T v)
    {
        uint8_t b[sizeof v];
        std::memcpy(b, &v, sizeof v);
        for (size_t i = 0; i < sizeof v; i++)
            bytes[at + i] = b[i];
    }
    void xor_f32(uint64_t at, float v, uint32_t key = 0x5a5a1234)
    {
        uint32_t u;
        std::memcpy(&u, &v, 4);
        put<uint32_t>(at, u ^ key);
        put<uint32_t>(at + 8, key);
    }
    bool operator()(uint64_t at, void *dst, size_t n)
    {
        for (size_t i = 0; i < n; i++) {
            auto it = bytes.find(at + i);
            if (it == bytes.end())
                return false;
            static_cast<uint8_t *>(dst)[i] = it->second;
        }
        return true;
    }
};

constexpr uint64_t LAPTIMES = 0x10000, MSG = 0x20000, LIST = 0x30000, ENTRY = 0x40000;

static Fake lap_memory(uint8_t car, uint8_t idx, float lap, float s1, float s2, float s3, bool lap_ok = true,
                       bool s_ok = true)
{
    Fake m;
    m.put<uint64_t>(LAPTIMES + lr::LT_CAR_DATA, LIST);
    m.put<uint64_t>(LIST + uint64_t(car) * 8, ENTRY);
    m.put<uint8_t>(MSG + lr::MSG_CAR, car);
    m.put<uint32_t>(MSG + lr::MSG_LAP, idx);
    m.put<uint8_t>(ENTRY + lr::REC_INDEX, idx);
    uint64_t base = ENTRY + uint64_t(idx) * lr::REC_STRIDE;
    m.xor_f32(base + lr::REC_S1, s1);
    m.xor_f32(base + lr::REC_S2, s2);
    m.xor_f32(base + lr::REC_S3, s3);
    m.xor_f32(base + lr::REC_LAP, lap);
    m.put<uint8_t>(base + lr::REC_LAP_KEY, lap_ok ? 1 : 0);
    m.put<uint64_t>(base + lr::REC_LAP_BITS, 0x1235);
    for (int i = 0; i < 3; i++) {
        m.put<uint8_t>(base + lr::REC_SECTOR_KEY[i], s_ok ? 1 : 0);
        m.put<uint64_t>(base + lr::REC_SECTOR_BITS[i], 0x77);
    }
    return m;
}

static void game_laps()
{
    {
        Fake m = lap_memory(2, 3, 91.234f, 29.001f, 31.002f, 31.231f);
        lr::GameLap g = lr::decode_lap(m, LAPTIMES, MSG);
        lr::sanity(g);
        expect(g.read && g.car == 2 && g.msg_lap == 3 && g.record == 3, "the record of the lap just completed is found by the entry's own index");
        expect(g.time_ms == int64_t(int32_t(91.234f * 1000.0f)) && g.sectors_ms.size() == 3 &&
                   g.sectors_ms[0] == int64_t(int32_t(29.001f * 1000.0f)),
               "the XOR pairs decode to the game's own millisecond truncation");
        expect(g.lap_valid && g.sector_valid[0] && g.sector_valid[1] && g.sector_valid[2],
               "the validity bit equal to the key is a valid lap and valid sectors");
        expect(g.usable, "a plausible record is trusted");
    }
    {
        Fake m = lap_memory(0, 0, 80.5f, 20.0f, 30.0f, 30.5f, false, false);
        lr::GameLap g = lr::decode_lap(m, LAPTIMES, MSG);
        lr::sanity(g);
        expect(g.read && !g.lap_valid && !g.sector_valid[0] && g.usable, "an invalid lap reads as invalid and stays usable");
    }
    {
        Fake m = lap_memory(1, 5, 91.234f, 29.001f, 31.002f, 31.231f);
        m.put<uint8_t>(MSG + lr::MSG_CAR, 9);
        lr::GameLap g = lr::decode_lap(m, LAPTIMES, MSG);
        expect(!g.read && !g.usable && !g.why.empty(), "a car with no record entry is refused");
        Fake n = lap_memory(1, 5, 91.234f, 29.001f, 31.002f, 31.231f);
        n.bytes.erase(ENTRY + 5 * lr::REC_STRIDE + lr::REC_LAP + 8);
        lr::GameLap h = lr::decode_lap(n, LAPTIMES, MSG);
        expect(!h.read, "a field that cannot be read refuses the whole record");
        Fake none;
        expect(!lr::decode_lap(none, LAPTIMES, MSG).read && !lr::decode_lap(none, 0, 0).read, "unreadable memory reads nothing");
    }
    struct Case {
        const char *what;
        float lap, s1, s2, s3;
    } bad[] = {
        {"a lap under 5 s", 4.0f, 1.0f, 1.5f, 1.5f},
        {"a lap over an hour", 4000.0f, 1000.0f, 1000.0f, 2000.0f},
        {"a NaN lap", std::nanf(""), 20.0f, 30.0f, 30.0f},
        {"an infinite lap", INFINITY, 20.0f, 30.0f, 30.0f},
        {"a zero sector", 80.0f, 0.0f, 40.0f, 40.0f},
        {"a negative sector", 80.0f, -1.0f, 41.0f, 40.0f},
        {"sectors that do not add up", 91.234f, 29.0f, 31.0f, 31.3f},
    };
    for (auto &c : bad) {
        Fake m = lap_memory(0, 1, c.lap, c.s1, c.s2, c.s3);
        lr::GameLap g = lr::decode_lap(m, LAPTIMES, MSG);
        lr::sanity(g);
        expect(g.read && !g.usable && !g.why.empty(), (std::string("sanity refuses ") + c.what).c_str());
    }
    {
        Fake m = lap_memory(0, 1, 91.234f, 29.0f, 31.0f, 31.2f, false, false);
        lr::GameLap g = lr::decode_lap(m, LAPTIMES, MSG);
        lr::sanity(g);
        expect(g.usable, "sectors that do not add up are only checked when the lap and all three sectors are valid");
        Fake edge = lap_memory(0, 1, 91.234f, 29.0f, 31.0f, 31.225f);
        lr::GameLap e = lr::decode_lap(edge, LAPTIMES, MSG);
        lr::sanity(e);
        expect(e.usable, "a sum within 10 ms of the lap time passes");
    }
}

constexpr uint64_t TLAP = 0x50000, CFG = 0x51000, MAP = 0x60000, SLOTS = 0x61000, COMP = 0x62000, VEC = 0x63000,
                   KEY = 0x1461026a8, LIST_OWNER = 0x70000, PARTS = 0x71000;

static Fake tlap_memory(float lap, uint32_t perf, uint32_t input)
{
    Fake m;
    m.put<float>(TLAP + lr::TL_LAP, lap);
    m.put<uint64_t>(TLAP + lr::TL_CFG, CFG);
    m.put<uint32_t>(CFG + lr::CFG_PERFORMANCE, perf);
    m.put<uint32_t>(CFG + lr::CFG_INPUT, input);
    m.put<uint8_t>(TLAP + lr::TL_BRK, 1);
    m.put<uint8_t>(TLAP + lr::TL_ABS, 1);
    m.put<uint8_t>(TLAP + lr::TL_TRC, 2);
    m.put<uint8_t>(TLAP + lr::TL_DRL, 0);
    m.put<uint8_t>(TLAP + lr::TL_GBX, 3);
    m.put<uint8_t>(TLAP + lr::TL_ERS, 1);
    m.put<uint64_t>(MAP + lr::MAP_SLOTS, SLOTS);
    m.put<uint64_t>(MAP + lr::MAP_MASK, 3);
    for (uint64_t i = 0; i < 4; i++) {
        m.put<uint64_t>(SLOTS + i * 16, i == 2 ? KEY : 0x99 + i);
        m.put<uint64_t>(SLOTS + i * 16 + 8, i == 2 ? COMP : 0);
    }
    m.put<uint64_t>(COMP + lr::COMP_VEC_BEGIN, VEC);
    m.put<uint64_t>(COMP + lr::COMP_VEC_END, VEC + 0x28);
    m.put<uint64_t>(VEC + lr::COMP_LAST_LAP_SLOT, TLAP);
    return m;
}

static void tlap_and_stats()
{
    Fake m = tlap_memory(91.234f, 1, 2);
    uint64_t t = lr::find_last_tlap(m, MAP, KEY);
    expect(t == TLAP, "the telemetry lap is found in the component map by its key");
    lr::TLapExtras x = lr::decode_tlap(m, t, int64_t(int32_t(91.234f * 1000.0f)));
    expect(x.ok && x.brk == 1 && x.abs == 1 && x.trc == 2 && x.drl == 0 && x.gbx == 3 && x.ers == 1 && x.input == 2 &&
               x.performance == 1,
           "assists, input device and performance state decode from the lap object");
    expect(std::string(lr::input_name(x.input)) == "keyboard" && std::string(lr::performance_name(x.performance)) == "equal" &&
               std::string(lr::performance_name(0)) == "realistic",
           "the codes have names");
    expect(!lr::decode_tlap(m, t, 80000).ok, "a telemetry lap whose time is not the lap just completed is refused");
    expect(lr::find_last_tlap(m, MAP, KEY + 8) == 0, "a key the map does not hold finds nothing");
    Fake badmask = tlap_memory(91.234f, 0, 0);
    badmask.put<uint64_t>(MAP + lr::MAP_MASK, 4);
    expect(lr::find_last_tlap(badmask, MAP, KEY) == 0, "a map whose size is not a power of two is refused");
    Fake shortvec = tlap_memory(91.234f, 0, 0);
    shortvec.put<uint64_t>(COMP + lr::COMP_VEC_END, VEC + 0x10);
    expect(lr::find_last_tlap(shortvec, MAP, KEY) == 0, "a component with fewer than four laps finds nothing");
    Fake p2 = tlap_memory(91.234f, 2, 0), p3 = tlap_memory(91.234f, 0, 4);
    expect(!lr::decode_tlap(p2, TLAP, 91234).ok && !lr::decode_tlap(p3, TLAP, 91234).ok,
           "a performance state or input device out of range is refused");

    Fake p;
    p.put<uint64_t>(LIST_OWNER + lr::PART_BEGIN, PARTS);
    p.put<uint64_t>(LIST_OWNER + lr::PART_END, PARTS + 2 * lr::PART_STRIDE);
    for (uint64_t i = 0; i < 2; i++) {
        p.put<uint8_t>(PARTS + i * lr::PART_STRIDE + lr::PART_CAR, uint8_t(i + 4));
        p.put<uint32_t>(PARTS + i * lr::PART_STRIDE + lr::PART_TEAM, uint32_t(100 + i));
        p.put<uint32_t>(PARTS + i * lr::PART_STRIDE + lr::PART_VEHICLE, uint32_t(200 + i));
        p.put<uint32_t>(PARTS + i * lr::PART_STRIDE + lr::PART_LIVERY, uint32_t(300 + i));
    }
    lr::Stats s = lr::find_stats(p, LIST_OWNER, 5);
    expect(s.ok && s.team == 101 && s.vehicle == 201 && s.livery == 301, "the participant of a car gives its team, vehicle and livery stat ids");
    expect(!lr::find_stats(p, LIST_OWNER, 9).ok, "a car with no participant gives nothing");
    p.put<uint64_t>(LIST_OWNER + lr::PART_END, PARTS + 100);
    expect(!lr::find_stats(p, LIST_OWNER, 5).ok, "a list that is not a whole number of entries is refused");
}

static lr::Lap fake_lap(uint32_t index, double ended)
{
    lr::Lap l;
    l.index = index;
    l.ended_s = ended;
    l.started_s = ended - 90;
    l.count = 100;
    return l;
}

static lr::GameLap fake_game(bool valid, bool usable = true)
{
    lr::GameLap g;
    g.read = true;
    g.usable = usable;
    g.lap_valid = valid;
    g.time_ms = 90000;
    return g;
}

static void sealing()
{
    {
        lr::Sealer s;
        s.expect(false, false);
        s.lap(fake_lap(1, 10), 10);
        auto out = s.poll(10);
        expect(out.size() == 1 && !out[0].game && out[0].ghost.empty() && !out[0].timed_out,
               "with no game hook a lap is released at once, as the clock path does");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.lap(fake_lap(1, 10), 10);
        expect(s.poll(10.5).empty(), "a lap waits for the game's result");
        s.game(fake_game(false), 10.1);
        auto out = s.poll(10.6);
        expect(out.size() == 1 && out[0].game && !out[0].game->lap_valid && out[0].ghost.empty() && !out[0].timed_out,
               "an invalid lap is released when its result arrives, with no wait for a ghost");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.game(fake_game(true), 9.95);
        s.lap(fake_lap(1, 10), 10);
        expect(s.poll(10.1).empty(), "a valid lap waits for a ghost after its result");
        s.ghost({1, 2, 3}, 10.3);
        auto out = s.poll(10.31);
        expect(out.size() == 1 && out[0].ghost.size() == 3 && out[0].game && !out[0].timed_out,
               "a ghost that arrives releases the lap at once with the blob");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.lap(fake_lap(1, 10), 10);
        s.game(fake_game(true), 10.05);
        expect(s.poll(11.9).empty(), "without a ghost the lap waits for the bound");
        auto out = s.poll(12.0);
        expect(out.size() == 1 && out[0].ghost.empty() && out[0].game && out[0].timed_out,
               "after the bound the lap is released without a ghost");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.lap(fake_lap(1, 10), 10);
        auto out = s.poll(12.0);
        expect(out.size() == 1 && !out[0].game && out[0].timed_out, "a result that never arrives releases the lap for the clock");
    }
    {
        lr::Sealer s;
        s.expect(true, false);
        s.lap(fake_lap(1, 10), 10);
        s.game(fake_game(true), 10.1);
        expect(s.poll(10.1).size() == 1, "with no ghost hook a valid lap does not wait for one");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.lap(fake_lap(1, 10), 10);
        s.game(fake_game(true, false), 10.1);
        auto out = s.poll(10.1);
        expect(out.empty(), "a refused result still lets a ghost arrive for a valid lap");
        s.ghost({9}, 10.2);
        out = s.poll(10.2);
        expect(out.size() == 1 && out[0].game && !out[0].game->usable && out[0].ghost.size() == 1, "an unusable result is passed on as unusable");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.lap(fake_lap(1, 10), 10);
        s.game(fake_game(false), 10.0);
        s.lap(fake_lap(2, 100), 100);
        s.game(fake_game(true), 100.0);
        s.ghost({7, 7}, 100.5);
        auto out = s.poll(100.5);
        expect(out.size() == 2 && out[0].lap.index == 1 && out[0].ghost.empty() && out[1].lap.index == 2 && out[1].ghost.size() == 2,
               "results and ghosts pair with the lap they belong to");
    }
    {
        lr::Sealer s;
        s.expect(true, true);
        s.game(fake_game(true), 50);
        s.ghost({1}, 50.2);
        s.poll(51);
        s.poll(54.3);
        expect(s.stale_games() == 1 && s.stale_ghosts() == 1 && s.waiting() == 0,
               "a result and a ghost with no lap are dropped and counted");
        s.lap(fake_lap(3, 60), 60);
        s.game(fake_game(true), 60.0);
        s.lap(fake_lap(4, 61), 61);
        auto out = s.flush();
        expect(out.size() == 2 && out[0].timed_out && s.waiting() == 0, "flush releases every waiting lap at the session end");
    }
}

int main()
{
    game_laps();
    tlap_and_stats();
    sealing();
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
