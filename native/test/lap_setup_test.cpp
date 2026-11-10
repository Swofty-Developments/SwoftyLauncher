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
    m.set<uint32_t>(AERO + ls::AERO_FRONT, 23);
    m.set<uint32_t>(AERO + ls::AERO_REAR, 23);
    m.set<float>(TUNE + 0x214, 0.2222f);
    m.set<float>(TUNE + 0x218, 0.5f);
    m.set<float>(TUNE + 0x228, 0.1f);
    m.set<float>(TUNE + 0x22c, 0.1f);
    m.set<float>(TUNE + 0x230, 0.4f);
    m.set<float>(TUNE + 0x234, 0.5333f);
    m.set<float>(TUNE + 0x238, 0.9f);
    m.set<float>(TUNE + 0x23c, 0.45f);
    m.set<float>(TUNE + 0x250, 0.7f);
    m.set<float>(TUNE + 0x254, 0.4f);
    m.set<float>(TUNE + 0x220, 0.5f);
    m.set<float>(TUNE + 0x248, 0.5f);
    m.set<float>(TUNE + 0x24c, 0.5f);
    m.set<float>(BRAKE + ls::BRAKE_PRESSURE_AT, 0.97f);
    m.set<float>(BRAKE + ls::BRAKE_BIAS_AT, 0.4f);
    m.set<float>(FUEL + ls::FUEL_AT, 10.0f);
    const float psi[4] = {21.3f, 21.3f, 24.2f, 24.2f};
    for (int i = 0; i < 4; i++) {
        m.set<uint64_t>(SUSP + ls::WHEELS + 8 * i, WHEEL + 0x1000 * i);
        m.set<float>(WHEEL + 0x1000 * i + ls::WHEEL_PRESSURE, psi[i] * 6.894757f);
    }
    return m;
}

int main()
{
    Mem m = good();
    ls::Setup s;
    expect(ls::read(m, CAR, s), "the proof-lap car reads");
    struct Truth {
        ls::Key k;
        double v;
    };
    // The stock F1 25 balanced preset (f1_2025_vehicle_package, steps 23 23 4 9 1 1 8 8 36 18 14 8 10 10 17 8 5 5) in UI units.
    const Truth truth[] = {{ls::FRONT_WING, 23},  {ls::REAR_WING, 23},     {ls::DIFF_ON, 30},          {ls::DIFF_OFF, 55},
                           {ls::FRONT_CAMBER, -3.4}, {ls::REAR_CAMBER, -1.9}, {ls::FRONT_TOE, 0.08},     {ls::REAR_TOE, 0.18},
                           {ls::FRONT_SUSPENSION, 37}, {ls::REAR_SUSPENSION, 19}, {ls::FRONT_ARB, 15},   {ls::REAR_ARB, 9},
                           {ls::FRONT_RIDE, 25},  {ls::REAR_RIDE, 50},     {ls::BRAKE_PRESSURE, 97},   {ls::BRAKE_BIAS, 58},
                           {ls::ENGINE_BRAKING, 50}, {ls::FUEL_LOAD, 10},  {ls::TYRE_FL, 24.2},        {ls::TYRE_FR, 24.2},
                           {ls::TYRE_RL, 21.3},   {ls::TYRE_RR, 21.3}};
    for (auto &t : truth) {
        char what[96];
        std::snprintf(what, sizeof what, "%s is %g in UI units (got %g)", ls::NAMES[t.k], t.v, s.v[t.k]);
        expect(s.has[t.k] && std::fabs(s.v[t.k] - t.v) < 0.011, what);
    }
    s.changed = false;
    auto j = ls::to_json(s);
    expect(j.type() == swofty::json::Type::object && j["front_wing"].as_int() == 23 && j["changed"].as_bool(true) == false, "json object");
    expect(j["diff_on_throttle"].type() == swofty::json::Type::integer && j["front_camber"].type() == swofty::json::Type::floating,
           "integers where the UI shows integers");

    Mem bar = good();
    bar.set<float>(WHEEL + ls::WHEEL_PRESSURE, 1.6692f);
    ls::Setup k;
    expect(ls::read(bar, CAR, k) && std::fabs(k.v[ls::TYRE_RL] - 24.2) < 0.05, "a bar reading converts to psi");

    Mem broken = good();
    broken.set<float>(WHEEL + ls::WHEEL_PRESSURE, 7000.0f);
    ls::Setup nb;
    expect(ls::read(broken, CAR, nb) && !nb.has[ls::TYRE_RL] && ls::diag_text(nb).find("tyre pressures unreadable") != std::string::npos,
           "an unreadable tyre pressure is named in the diag line");

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
    expect(ls::diag_text(s).find("front_wing=23") != std::string::npos, "diag text names raw values");

    namespace lf = runtime::lap_file;
    lf::Header h;
    h.time_ms = 90000;
    h.setup = ls::to_json(s);
    lf::Layout l;
    l.add("t", lf::Type::f32);
    auto hj = lf::header_json(h, l);
    expect(hj["setup"]["rear_wing"].as_int() == 23, "the header carries the setup object");
    lf::Header none_h;
    expect(lf::header_json(none_h, l)["setup"].type() != swofty::json::Type::object, "no setup object when unreadable");
    if (failed) {
        std::printf("%d failed\n", failed);
        return 1;
    }
    std::printf("all passed\n");
}
