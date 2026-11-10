#include "lap_file_logic.hpp"
#include "lap_setup_logic.hpp"

#include <cstdio>
#include <map>
#include <vector>

namespace ls = runtime::lap_setup;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Mem {
    std::map<uint64_t, std::vector<uint8_t>> blocks;
    template <class T> void set(uint64_t at, T v)
    {
        auto &b = blocks[at & ~0xffffull];
        if (b.empty())
            b.assign(0x10000, 0);
        std::memcpy(b.data() + (at & 0xffff), &v, sizeof v);
    }
    bool operator()(uint64_t at, void *out, size_t n)
    {
        auto it = blocks.find(at & ~0xffffull);
        if (it == blocks.end() || (at & 0xffff) + n > 0x10000)
            return false;
        std::memcpy(out, it->second.data() + (at & 0xffff), n);
        return true;
    }
};

static const uint64_t CAR = 0x100000, TUNE = 0x200000, AERO = 0x300000, BRAKE = 0x400000, FUEL = 0x500000, SUSP = 0x600000,
                      WHEEL = 0x700000;

static Mem good()
{
    Mem m;
    m.set<uint64_t>(CAR + ls::CAR_TUNING, TUNE);
    m.set<uint64_t>(CAR + ls::CAR_AERO, AERO);
    m.set<uint64_t>(CAR + ls::CAR_BRAKE, BRAKE);
    m.set<uint64_t>(CAR + ls::CAR_FUEL, FUEL);
    m.set<uint64_t>(CAR + ls::CAR_SUSP, SUSP);
    m.set<uint32_t>(CAR + ls::CAR_HANDLING, 8);
    m.set<uint32_t>(AERO + ls::AERO_FRONT, 25);
    m.set<uint32_t>(AERO + ls::AERO_REAR, 30);
    m.set<float>(TUNE + 0x214, 0.5f);
    m.set<float>(TUNE + 0x30, 0.1f);
    m.set<float>(TUNE + 0x34, 1.0f);
    m.set<float>(TUNE + 0x218, 1.0f);
    m.set<float>(TUNE + 0x38, 0.1f);
    m.set<float>(TUNE + 0x3c, 1.0f);
    m.set<float>(TUNE + 0x228, 0.0f);
    m.set<float>(TUNE + 0x68, -0.0610865f);
    m.set<float>(TUNE + 0x6c, -0.0436332f);
    m.set<float>(TUNE + 0x230, 0.5f);
    m.set<float>(TUNE + 0x78, 0.0f);
    m.set<float>(TUNE + 0x7c, 0.00872665f);
    m.set<float>(TUNE + 0x238, 0.5f);
    m.set<float>(TUNE + 0x250, 0.5f);
    m.set<float>(TUNE + 0x248, 0.25f);
    m.set<float>(TUNE + 0xa8, 0.030f);
    m.set<float>(TUNE + 0xac, 0.050f);
    m.set<float>(BRAKE + ls::BRAKE_PRESSURE_AT, 1.0f);
    m.set<float>(BRAKE + ls::BRAKE_BIAS_AT, 0.56f);
    m.set<float>(FUEL + ls::FUEL_AT, 30.0f);
    for (int i = 0; i < 4; i++) {
        m.set<uint64_t>(SUSP + ls::WHEELS + 8 * i, WHEEL + 0x1000 * i);
        m.set<float>(WHEEL + 0x1000 * i + ls::WHEEL_PRESSURE, i < 2 ? 21.0f : 22.5f);
    }
    return m;
}

int main()
{
    Mem m = good();
    ls::Setup s;
    expect(ls::read(m, CAR, s), "a populated car reads");
    expect(s.has[ls::FRONT_WING] && s.v[ls::FRONT_WING] == 25 && s.v[ls::REAR_WING] == 30, "wings are the levels");
    expect(std::fabs(s.v[ls::DIFF_ON] - 55) < 0.01 && std::fabs(s.v[ls::DIFF_OFF] - 100) < 0.01, "diff percent from the lerp");
    expect(std::fabs(s.v[ls::FRONT_CAMBER] - (-3.5)) < 0.01, "radian camber ranges become degrees");
    expect(std::fabs(s.v[ls::FRONT_TOE] - 0.25) < 0.01, "toe in degrees");
    expect(s.v[ls::FRONT_SUSPENSION] == 21 && s.v[ls::FRONT_ARB] == 11, "suspension and anti-roll bar steps are 1-based");
    expect(std::fabs(s.v[ls::FRONT_RIDE] - 35) < 0.01, "ride height in millimetres");
    expect(s.v[ls::BRAKE_PRESSURE] == 100 && s.v[ls::BRAKE_BIAS] == 56, "brake pressure and bias percent");
    expect(s.v[ls::FUEL_LOAD] == 30, "fuel in kg");
    expect(s.v[ls::TYRE_RL] == 21 && s.v[ls::TYRE_FR] == 22.5, "tyre pressures follow the wheel order RL RR FL FR");
    s.changed = false;
    auto j = ls::to_json(s);
    expect(j.type() == swofty::json::Type::object && j["front_wing"].as_int() == 25 && j["changed"].as_bool(true) == false, "json object");
    expect(std::fabs(j["front_camber"].as_double() + 3.5) < 0.01, "json camber");

    Mem kpa = good();
    kpa.set<float>(WHEEL + ls::WHEEL_PRESSURE, 150000.0f);
    ls::Setup k;
    expect(ls::read(kpa, CAR, k) && std::fabs(k.v[ls::TYRE_RL] - 21.756) < 0.01, "pascal tyre pressure converts to psi");

    Mem bad = good();
    bad.set<float>(TUNE + 0x214, 7.0f);
    bad.set<uint32_t>(AERO + ls::AERO_REAR, 5000);
    ls::Setup b;
    expect(ls::read(bad, CAR, b) && !b.has[ls::DIFF_ON] && !b.has[ls::REAR_WING], "out-of-range values drop their key only");

    Mem none;
    ls::Setup n;
    expect(!ls::read(none, CAR, n) && ls::to_json(n)["changed"].type() != swofty::json::Type::boolean, "an unreadable car gives nothing");
    Mem sparse;
    sparse.set<uint64_t>(CAR + ls::CAR_TUNING, TUNE);
    expect(!ls::read(sparse, CAR, n), "a car with fewer than four readable keys is dropped");

    ls::Setup a = s, c = s;
    expect(!ls::differs(a, c), "equal setups do not differ");
    c.v[ls::BRAKE_BIAS] = 57;
    expect(ls::differs(a, c), "a changed value differs");
    c = s;
    c.has[ls::FUEL_LOAD] = false;
    expect(ls::differs(a, c), "a missing key differs");
    expect(ls::diag_text(s).find("front_wing=25") != std::string::npos, "diag text names raw values");

    namespace lf = runtime::lap_file;
    lf::Header h;
    h.time_ms = 90000;
    h.setup = ls::to_json(s);
    lf::Layout l;
    l.add("t", lf::Type::f32);
    auto hj = lf::header_json(h, l);
    expect(hj["setup"]["rear_wing"].as_int() == 30, "the header carries the setup object");
    lf::Header none_h;
    expect(lf::header_json(none_h, l)["setup"].type() != swofty::json::Type::object, "no setup object when unreadable");
    if (failed) {
        std::printf("%d failed\n", failed);
        return 1;
    }
    std::printf("all passed\n");
}
