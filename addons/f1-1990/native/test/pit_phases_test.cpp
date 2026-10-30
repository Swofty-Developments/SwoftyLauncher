// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cmath>
#include <algorithm>
#include <cstdio>
#include <string>
#include <unordered_map>

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
        Plan plan = f1990::pit::plan(std258, rate, 30.f, std258.start);
        float held, slow;
        float t = run(plan, std258, 30.f, 1.f / 60.f, held, slow);
        float want = stock_time(std258, rate, 30.f);
        char name[96];
        std::snprintf(name, sizeof name, "258 at %.1f s: %.2f s, stock %.2f s (held %.2f, slow %.2f)", target, t, want, held,
                      slow);
        check(name, plan.valid && std::fabs(t - want) < 0.15f && held > 0.6f * plan.extra && slow > 0);
    }

    Phases flon{17, 155, 158, {77, 81, 83, 120}, 4, 146};
    Plan p2 = plan(flon, 0.5f, 30.f, flon.start);
    float held, slow;
    float t2 = run(p2, flon, 30.f, 1.f / 30.f, held, slow);
    check("mistake variant keeps the stock total", p2.valid && std::fabs(t2 - stock_time(flon, 0.5f, 30.f)) < 0.15f);

    check("rate 1 is left alone", !plan(std258, 1.0f, 30.f, 13).valid);
    check("faster than nominal is left alone", !plan(std258, 1.3f, 30.f, 13).valid);

    Phases noguns{13, 90, 98, {}, 0, 91};
    Plan p3 = plan(noguns, 0.3f, 30.f, noguns.start);
    float t3 = run(p3, noguns, 30.f, 1.f / 60.f, held, slow);
    check("no gun frames: all in the jack stretch", p3.valid && p3.holds == 0 && std::fabs(t3 - stock_time(noguns, 0.3f, 30.f)) < 0.15f);

    Phases nothing{13, 90, 98, {}, 0, -1};
    check("no gun and no jack frames: stock", !plan(nothing, 0.3f, 30.f, 13).valid);

    // The game's update, as the 1990 log shows it: the sequence time is in seconds (scale 1), the crew speed 0.252 is
    // already in base and rate when the wrap first looks at 0.5 s of F1StartJobDoStandardStop258 (Start 13/30 s).
    // Each update scales dt by rate on entry, advances the time, rewrites rate as factor x base; Stop puts both back
    // to 1. The wrap drives the plan from its first look, in the sequence's units.
    for (float factor : {1.0f, 1.5f}) {
        const float scale = 1.f, unit = scale / 30.f;
        Phases p_units = in_units(std258, unit);
        float r = 0.252f, base = r, rate = r, now = 0.5f, t = 0, dt = 1.f / 60.f, fac = 1, end_at = -1;
        bool active = false, stopped = false, begun = false;
        Plan p;
        float held = 0, first_rate = -1, max_hold_drift = 0, held_at = -1;
        while (t < 30) {
            if (!begun && base > 0.01f && base < 0.999f) {
                begun = true;
                p = plan(p_units, base, scale, now, unit);
                active = p.valid;
                fac = 1;
            }
            if (active) {
                active = drive(p, fac, now, dt, base, rate);
                if (rate == 0.f) {
                    held += dt;
                    if (held_at < 0)
                        held_at = now;
                    max_hold_drift = std::max(max_hold_drift, now - held_at);
                } else
                    held_at = -1;
                if (first_rate < 0)
                    first_rate = rate;
            }
            float before = now;
            now += dt * rate * scale;
            if (!stopped && before < p_units.stop && now >= p_units.stop) {
                stopped = true;
                base = rate = 1;
            }
            rate = factor * base;
            t += dt;
            if (now >= p_units.jc && end_at < 0)
                end_at = t;
            if (end_at >= 0 && !active)
                break;
        }
        float want = (p_units.stop - 0.5f) / (scale * r) + (p_units.jc - p_units.stop) / scale;
        char name[200];
        std::snprintf(name, sizeof name,
                      "258 in seconds from 0.5 s at 0.252, factor %.1f: speed 1 from the first update (%.2f), JC after "
                      "%.2f s (game %.2f s), held %.2f s, first hold at %.3f s, base back to %.1f",
                      factor, first_rate, end_at, want, held, p.holds ? p.hold_frame[0] : -1.f, base);
        check(name, p.valid && first_rate == 1.f && held > 4.f && base == 1.f && max_hold_drift < 1e-6f &&
                        std::fabs(p.hold_frame[0] - 77.f / 30.f) < 1e-4f && (factor != 1.f || std::fabs(end_at - want) < 0.1f));
    }

    std::unordered_map<std::string, Phases> table{{"startjobdostandardstop258", std258}, {"startjobdostandardstop21", flon}};
    check("exact name", find(table, "F1StartJobDoStandardStop258") == &table["startjobdostandardstop258"]);
    check("name with a suffix", find(table, "F1StartJobDoStandardStop258Ferrari") == &table["startjobdostandardstop258"]);
    check("same family", find(table, "F1StartJobDoStandardStop2585SP") == &table["startjobdostandardstop258"]);
    check("other family", find(table, "F1ReadyIdle") == nullptr);

    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures != 0;
}
