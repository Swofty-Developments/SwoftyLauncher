// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cmath>
#include <cstdio>

#include "pit_phases.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static float stock_time(const f1990::pit::Phases &p, float rate, float fps)
{
    return (p.stop - p.start) / (fps * rate) + std::max(0.f, p.jc - p.stop) / fps;
}

static float run(f1990::pit::Plan plan, const f1990::pit::Phases &p, float fps, float dt, float &held, float &slow)
{
    float frame = p.start, t = 0;
    held = slow = 0;
    while (frame < plan.end && t < 120) {
        float r = f1990::pit::step(plan, frame, dt);
        held += r == 0.f ? dt : 0;
        slow += r > 0.f && r < 1.f ? dt : 0;
        frame += dt * r * fps;
        t += dt;
    }
    return t;
}

int main()
{
    using namespace f1990::pit;
    check("key from eaid path", key("eaid://animation_package/sequences/pitstops/f1/pitstopstartjobdostandardstop258.sequence") ==
                                    "startjobdostandardstop258");
    check("key from sequence name", key("F1StartJobDoStandardStop258") == "startjobdostandardstop258");

    Phases std258{13, 90, 98, {77, 82, 83}, 3, 91};
    for (float target : {7.f, 9.5f, 12.2f}) {
        float rate = 2.58f / target;
        Plan plan = f1990::pit::plan(std258, rate, 30.f);
        float held, slow;
        float t = run(plan, std258, 30.f, 1.f / 60.f, held, slow);
        float want = stock_time(std258, rate, 30.f);
        char name[96];
        std::snprintf(name, sizeof name, "258 at %.1f s: %.2f s, stock %.2f s (held %.2f, slow %.2f)", target, t, want, held,
                      slow);
        check(name, plan.valid && std::fabs(t - want) < 0.15f && held > 0.6f * plan.extra && slow > 0);
    }

    Phases flon{17, 155, 158, {77, 81, 83, 120}, 4, 146};
    Plan p2 = plan(flon, 0.5f, 30.f);
    float held, slow;
    float t2 = run(p2, flon, 30.f, 1.f / 30.f, held, slow);
    check("mistake variant keeps the stock total", p2.valid && std::fabs(t2 - stock_time(flon, 0.5f, 30.f)) < 0.15f);

    check("rate 1 is left alone", !plan(std258, 1.0f, 30.f).valid);
    check("faster than nominal is left alone", !plan(std258, 1.3f, 30.f).valid);

    Phases noguns{13, 90, 98, {}, 0, 91};
    Plan p3 = plan(noguns, 0.3f, 30.f);
    float t3 = run(p3, noguns, 30.f, 1.f / 60.f, held, slow);
    check("no gun frames: all in the jack stretch", p3.valid && p3.holds == 0 && std::fabs(t3 - stock_time(noguns, 0.3f, 30.f)) < 0.15f);

    Phases nothing{13, 90, 98, {}, 0, -1};
    check("no gun and no jack frames: stock", !plan(nothing, 0.3f, 30.f).valid);

    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures != 0;
}
