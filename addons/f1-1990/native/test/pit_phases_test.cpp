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

    // The game's update: Start writes the crew speed into base and rate; each update scales dt by rate on entry,
    // advances the frame, rewrites rate as factor x base, and Stop puts both back to 1. The wrap drives the plan
    // before each update.
    for (float factor : {1.0f, 1.5f}) {
        float r = 2.58f / 9.5f, base = 1, rate = 1, frame = 0, t = 0, dt = 1.f / 60.f, fac = 1, stopped_at = -1;
        bool started = false, active = false, stopped = false;
        Plan p = plan(std258, r, 30.f);
        float held = 0;
        while (t < 30) {
            if (!active && started && base < 0.999f && frame < std258.start + 12) {
                active = true;
                fac = 1;
            }
            if (active) {
                active = drive(p, fac, frame, dt, base, rate);
                held += rate == 0.f ? dt : 0;
            }
            float scaled = dt * rate;
            float before = frame;
            frame += scaled * 30.f;
            if (!started && before < std258.start && frame >= std258.start) {
                started = true;
                base = rate = r;
            }
            if (!stopped && before < std258.stop && frame >= std258.stop) {
                stopped = true;
                base = rate = 1;
            }
            rate = factor * base;
            t += dt;
            if (frame >= std258.jc && stopped_at < 0)
                stopped_at = t;
            if (stopped_at >= 0 && !active)
                break;
        }
        float t0 = std258.start / 30.f;
        float want = t0 + (std258.stop - std258.start) / (30.f * r * factor) + (std258.jc - std258.stop) / (30.f * factor);
        char name[128];
        std::snprintf(name, sizeof name, "game loop, factor %.1f: JC at %.2f s (game %.2f s), held %.2f s, base back to %.1f",
                      factor, stopped_at, factor == 1.f ? want : stopped_at, held, base);
        check(name, held > 3.f && base == 1.f && (factor != 1.f || std::fabs(stopped_at - want) < 0.2f));
    }

    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures != 0;
}
