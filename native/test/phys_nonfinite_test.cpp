// Host-native unit test for the core's physics non-finite log (native/src/core/guards/engine/phys_nonfinite_logic.hpp).
// Built and run by native/test/run_tests.sh phys_nonfinite. The Wine harness (run_phys_nonfinite_test.sh) runs the same
// checker after the game's own Ne_PhysUpdateStage_FinishStepNotifyBodies on each game image it finds.

#include "core/guards/engine/phys_nonfinite_logic.hpp"

#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <string>
#include <vector>

namespace pn = runtime::phys_nonfinite;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static float from_bits(uint32_t bits)
{
    float f;
    std::memcpy(&f, &bits, sizeof f);
    return f;
}

constexpr size_t BODY_SIZE = 0x4b0;

struct World {
    alignas(16) unsigned char bytes[0x1200] = {};
    uintptr_t at() const { return reinterpret_cast<uintptr_t>(bytes); }
};

struct Pool {
    std::vector<unsigned char *> bodies;
    World world;

    explicit Pool(size_t n)
    {
        for (size_t i = 0; i < n; i++) {
            auto *b = static_cast<unsigned char *>(std::aligned_alloc(64, BODY_SIZE + 0x40));
            std::memset(b, 0, BODY_SIZE + 0x40);
            float pos[4] = {float(i), 1.f, -float(i), 1.f}, lin[4] = {10.f, 0.f, 72.f, 0.f}, ang[4] = {0.f, 0.5f, 0.f, 0.f};
            std::memcpy(b + pn::BODY_POSITION, pos, sizeof pos);
            std::memcpy(b + pn::BODY_LINEAR, lin, sizeof lin);
            std::memcpy(b + pn::BODY_ANGULAR, ang, sizeof ang);
            float inv_mass = 1.f / 798.f;
            std::memcpy(b + pn::BODY_INV_MASS, &inv_mass, sizeof inv_mass);
            bodies.push_back(b);
        }
        for (size_t i = 0; i < n; i++) {
            uintptr_t next = i + 1 < n ? reinterpret_cast<uintptr_t>(bodies[i + 1]) : 0;
            std::memcpy(bodies[i] + pn::BODY_NEXT, &next, sizeof next);
        }
        uintptr_t head = n ? reinterpret_cast<uintptr_t>(bodies[0]) : 0;
        std::memcpy(world.bytes + pn::WORLD_BODIES, &head, sizeof head);
        float dt = 1.f / 60.f;
        std::memcpy(world.bytes + pn::WORLD_STEP_TIME, &dt, sizeof dt);
    }
    ~Pool()
    {
        for (auto *b : bodies)
            std::free(b);
    }
    uintptr_t body(size_t i) const { return reinterpret_cast<uintptr_t>(bodies[i]); }
    void set(size_t i, uintptr_t off, int lane, float v) { std::memcpy(bodies[i] + off + 4 * lane, &v, sizeof v); }
    std::vector<unsigned char> snapshot() const
    {
        std::vector<unsigned char> out;
        for (auto *b : bodies)
            out.insert(out.end(), b, b + BODY_SIZE);
        out.insert(out.end(), world.bytes, world.bytes + sizeof world.bytes);
        return out;
    }
};

static std::vector<std::string> lines;
static uintptr_t chassis = 0;

static void sink(const std::string &line) { lines.push_back(line); }

static pn::Owner owner(uintptr_t body)
{
    pn::Owner o;
    o.table = true;
    if (body == chassis) {
        o.slot = 12;
        o.vehicle_id = 13;
        o.ai = true;
        o.wheels = true;
        for (int i = 0; i < 4; i++)
            o.corners[i] = {true, 80.f + float(i), 0.01f, i == 2 ? from_bits(0x7fc00000u) : 0.02f};
    }
    return o;
}

static size_t count_with(const std::string &needle)
{
    size_t n = 0;
    for (auto &l : lines)
        n += l.find(needle) != std::string::npos;
    return n;
}

int main()
{
    const float nan = from_bits(0x7fc00000u), neg_nan = from_bits(0xffc00000u), snan = from_bits(0x7f800001u);
    const float inf = std::numeric_limits<float>::infinity();

    {
        Pool p(1);
        expect(pn::fields(p.body(0)) == 0, "a body with finite position and velocities has no field that is not finite");
        for (float v : {0.f, -0.f, FLT_MAX, -FLT_MAX, FLT_MIN, from_bits(1u), 1e30f}) {
            p.set(0, pn::BODY_LINEAR, 0, v);
            p.set(0, pn::BODY_ANGULAR, 1, v);
            p.set(0, pn::BODY_POSITION, 2, v);
        }
        expect(pn::fields(p.body(0)) == 0, "zero, the largest and smallest floats and denormals are finite");
        p.set(0, pn::BODY_LINEAR, 3, nan);
        p.set(0, pn::BODY_ANGULAR, 3, inf);
        p.set(0, pn::BODY_POSITION, 3, nan);
        expect(pn::fields(p.body(0)) == 0, "the fourth lane of each vector is not read");
    }
    struct Case {
        uintptr_t off;
        uint8_t field;
        const char *name;
    };
    const Case cases[] = {{pn::BODY_LINEAR, pn::linear, "linear velocity"},
                          {pn::BODY_ANGULAR, pn::angular, "angular velocity"},
                          {pn::BODY_POSITION, pn::position, "position"}};
    bool each = true;
    for (const Case &c : cases)
        for (int lane = 0; lane < 3; lane++)
            for (float v : {nan, neg_nan, snan, inf, -inf}) {
                Pool p(1);
                p.set(0, c.off, lane, v);
                each = each && pn::fields(p.body(0)) == c.field;
            }
    expect(each, "a NaN of either sign, a signalling NaN or an infinity in any lane of any field names that field alone");

    {
        Pool p(500);
        auto before = p.snapshot();
        pn::Walk w = pn::walk(*reinterpret_cast<const uintptr_t *>(p.world.at() + pn::WORLD_BODIES));
        expect(w.bodies == 500 && !w.nonfinite && !w.unended, "a walk over 500 finite bodies reads 500 and finds none");
        expect(p.snapshot() == before, "the walk writes nothing to the bodies or the world");
        p.set(499, pn::BODY_POSITION, 2, nan);
        w = pn::walk(*reinterpret_cast<const uintptr_t *>(p.world.at() + pn::WORLD_BODIES));
        expect(w.nonfinite && w.bodies == 500, "the last body's position NaN is found");
        p.set(499, pn::BODY_POSITION, 2, 1.f);
        p.set(0, pn::BODY_ANGULAR, 0, -inf);
        w = pn::walk(*reinterpret_cast<const uintptr_t *>(p.world.at() + pn::WORLD_BODIES));
        expect(w.nonfinite, "the first body's angular infinity is found");
    }
    {
        Pool p(3);
        uintptr_t loop = p.body(0);
        std::memcpy(p.bodies[2] + pn::BODY_NEXT, &loop, sizeof loop);
        pn::Walk w = pn::walk(p.body(0));
        expect(w.unended && w.bodies == pn::MAX_BODIES, "a body list that loops stops at the cap and says it did not end");
        expect(pn::walk(0).bodies == 0 && !pn::walk(0).nonfinite, "an empty list reads no body");
    }

    {
        Pool p(40);
        chassis = p.body(7);
        pn::Checker checker{&sink, &owner};
        uint64_t ticks = 0;
        lines.clear();
        auto before = p.snapshot();
        checker.step(p.world.at(), ticks);
        expect(lines.size() == 1 && count_with("finished its first step (step 1) with 40 bodies") == 1,
               "the first step of a world logs its body count once");
        checker.step(p.world.at(), ticks);
        expect(lines.size() == 1, "a second finite step logs nothing");
        expect(p.snapshot() == before, "finite steps leave every body and the world as they were");

        p.set(7, pn::BODY_LINEAR, 1, nan);
        checker.step(p.world.at(), ticks);
        expect(lines.size() == 2, "a body whose linear velocity turns NaN is logged");
        const std::string &l = lines.back();
        expect(l.find("[phys] step 3: body 7 of 40") == 0, "the line starts with the step, the body's index and the count");
        expect(l.find("s): linear velocity is not finite; position (7, 1, -7)") != std::string::npos, "the line names the field");
        expect(l.find("linear velocity (10, nan(0x7fc00000), 72)") != std::string::npos, "the line gives the NaN's bits");
        expect(l.find("the chassis of vehicle 13 (table slot 12, AI)") != std::string::npos, "the line names the car");
        expect(l.find("FL 82/0.01/nan(0x7fc00000)") != std::string::npos, "the line gives each wheel's spin and slip");
        expect(l.find("1 body in this step is not finite") != std::string::npos, "the line counts the bodies of the step");
        for (int i = 0; i < 100; i++)
            checker.step(p.world.at(), ticks);
        expect(lines.size() == 2, "the same body stays quiet over the next 100 steps");

        p.set(8, pn::BODY_ANGULAR, 2, inf);
        p.set(8, pn::BODY_POSITION, 0, -inf);
        checker.step(p.world.at(), ticks);
        expect(lines.size() == 3 && count_with("body 8 of 40") == 1 &&
                   count_with("angular velocity and position are not finite") == 1,
               "a second body with two fields that are not finite is logged once, naming both");
        expect(count_with("no car's chassis") == 1, "a body that is no car's chassis says so");
        expect(lines.back().find("2 bodies in this step are not finite") != std::string::npos,
               "the second line counts both bodies");

        before = p.snapshot();
        checker.step(p.world.at(), ticks);
        expect(p.snapshot() == before, "a step that finds known bodies that are not finite writes nothing");

        for (size_t i = 10; i < 40; i++)
            p.set(i, pn::BODY_POSITION, 1, nan);
        checker.step(p.world.at(), ticks);
        expect(checker.logged() == size_t(pn::LOG_BODIES) && lines.size() == 1 + size_t(pn::LOG_BODIES),
               "body lines stop at the cap");
        expect(lines.back().find("(further non-finite bodies are not logged)") != std::string::npos,
               "the last body line says further bodies are not logged");
        checker.step(p.world.at(), ticks);
        expect(lines.size() == 1 + size_t(pn::LOG_BODIES) && checker.unlogged_steps() == 1,
               "past the cap a step is only counted");

        pn::Window w = checker.take();
        expect(w.steps == checker.steps() && w.max_bodies == 40 && w.bodies == 40 * w.steps,
               "the window counts every step and its bodies");
        expect(w.nonfinite_steps == w.steps - 2, "every step from the first NaN on counts as one that found a body");
        expect(checker.take().steps == 0, "taking the window starts a new one");
    }

    {
        std::vector<Pool *> worlds;
        for (int i = 0; i < pn::LOG_WORLDS + 2; i++)
            worlds.push_back(new Pool(3));
        pn::Checker checker{&sink, &owner};
        uint64_t ticks = 0;
        lines.clear();
        for (int round = 0; round < 20; round++)
            for (int i = 0; i < 2; i++)
                checker.step(worlds[i]->world.at(), ticks);
        expect(lines.size() == 2 && count_with("finished its first step (step 1)") == 1 &&
                   count_with("finished its first step (step 2)") == 1,
               "two worlds that take turns log their first step once each");
        for (auto *w : worlds)
            checker.step(w->world.at(), ticks);
        expect(lines.size() == size_t(pn::LOG_WORLDS) &&
                   lines.back().find("(further worlds are not logged)") != std::string::npos,
               "first-step lines stop at the world cap");
        worlds[0]->set(1, pn::BODY_LINEAR, 0, nan);
        worlds[1]->set(2, pn::BODY_ANGULAR, 0, nan);
        for (int round = 0; round < 5; round++)
            for (int i = 0; i < 2; i++)
                checker.step(worlds[i]->world.at(), ticks);
        expect(lines.size() == size_t(pn::LOG_WORLDS) + 2 && checker.logged() == 2,
               "a body that is not finite in each of two worlds is logged once each");
        for (auto *w : worlds)
            delete w;
    }

    {
        pn::Window w;
        w.steps = 3600;
        w.bodies = 3600 * 210;
        w.ticks = 3600 * 4000;
        w.max_ticks = 20000;
        w.max_bodies = 230;
        pn::Rates r = pn::rates(w, 60, 0.25);
        expect(std::fabs(r.ns_per_step - 1000) < 1e-6 && std::fabs(r.max_ns - 5000) < 1e-6, "ticks become nanoseconds");
        expect(std::fabs(r.steps_per_s - 60) < 1e-9 && std::fabs(r.bodies_per_step - 210) < 1e-9, "steps per second and bodies");
        expect(std::fabs(r.us_per_s_60 - 60) < 1e-9 && std::fabs(r.us_per_s_240 - 240) < 1e-9,
               "1000 ns per step costs 60 us per s at 60 steps/s and 240 us per s at 240");
        std::string line = pn::perf_line(w, 60, 0.25);
        expect(line ==
                   "[perf] physics non-finite check: last 60 s: 3600 steps (60.0 per s), 210 bodies per step (max 230), 1000 ns "
                   "per step (max 5000 ns): 60.0 us per s at 60 steps/s (0.0060% of a 16.7 ms frame), 240.0 us per s at 240 "
                   "steps/s (0.0240% of a 4.2 ms frame); 0 steps found a body that is not finite",
               "the perf line");
        expect(pn::rates(pn::Window{}, 60, 0.25).ns_per_step == 0, "an empty window costs nothing");
    }

    expect(pn::value(1.5f) == "1.5" && pn::value(-inf) == "-inf" && pn::value(from_bits(0xffc00000u)) == "nan(0xffc00000)",
           "values print finite, infinite and NaN with its bits");
    expect(pn::field_names(pn::linear | pn::position) == "linear velocity and position", "field names join");
    expect(pn::owner_text(pn::Owner{}) == "the car table is not read on this build", "an unread car table says so");
    expect(pn::bp::proven_on(pn::BODY_LIST, pn::bp::Build::kb) && pn::bp::proven_on(pn::BODY_LIST, pn::bp::Build::ea) &&
               pn::bp::proven_on(pn::BODY_LIST, pn::bp::Build::steam_1_0_145) &&
               pn::bp::proven_on(pn::BODY_LIST, pn::bp::Build::ea_1_0_145) &&
               !pn::bp::proven_on(pn::BODY_LIST, pn::bp::Build::unknown),
           "the layout is proven on the four known builds and no other");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
