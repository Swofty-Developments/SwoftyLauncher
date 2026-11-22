// Host-native unit test for the extra box hold (native/src/core/hooks/pit_hold_logic.hpp): the hold is fixed at pit
// entry, added only in the box, served on the game's exits from the box and dropped on any other exit; the crew's
// stop-go clip answer, the stretched and frozen wait, the fallback to a freeze when the clip is missing, and the
// clean-up of bad numbers. Built and run by native/test/run_tests.sh pit_hold.

#include "core/hooks/pit_hold_logic.hpp"

#include <cmath>
#include <cstdio>

namespace ph = swofty_core::hooks::pit_hold;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

static bool near(float a, float b) { return std::fabs(a - b) < 1e-4f; }

static void enter_box(ph::Table &t, uint32_t c)
{
    t.on_status(c, ph::ON_TRACK, ph::ENTERING_PIT_LANE);
    t.on_status(c, ph::ENTERING_PIT_LANE, ph::ENTERING_PIT_BOX);
    t.on_status(c, ph::ENTERING_PIT_BOX, ph::IN_PIT_BOX);
}

static void hold()
{
    ph::Table t;
    t.set_crew(ph::CREW_NONE);
    CHECK(t.box_extra(3) == 0);
    CHECK(t.arm(3, 7.5f));
    CHECK(!t.arm(ph::CARS, 1));
    CHECK(t.box_extra(3) == 0);
    t.on_status(3, ph::ON_TRACK, ph::ENTERING_PIT_LANE);
    CHECK(t.box_extra(3) == 0);
    t.arm(3, 20);
    t.on_status(3, ph::ENTERING_PIT_LANE, ph::ENTERING_PIT_BOX);
    t.on_status(3, ph::ENTERING_PIT_BOX, ph::IN_PIT_BOX);
    CHECK(near(t.box_extra(3), 7.5f));
    CHECK(t.box_extra(4) == 0);
    float s = t.on_status(3, ph::IN_PIT_BOX, ph::PERFORMED_RELEASE);
    CHECK(near(s, 7.5f));
    CHECK(t.box_extra(3) == 0);
    ph::Snapshot n = t.snapshot(3);
    CHECK(n.serves == 1 && near(n.served, 7.5f) && n.armed == 0 && n.stop == 0);
    t.on_status(3, ph::PERFORMED_RELEASE, ph::LEAVING_PIT_BOX);
    CHECK(t.snapshot(3).serves == 1);

    t.arm(5, 3);
    enter_box(t, 5);
    CHECK(near(t.on_status(5, ph::IN_PIT_BOX, ph::CREW_COMPLETED), 3));
    CHECK(t.on_status(5, ph::CREW_COMPLETED, ph::PERFORMED_RELEASE) == 0);
    CHECK(t.snapshot(5).serves == 1);

    t.arm(6, 10);
    enter_box(t, 6);
    CHECK(t.on_status(6, ph::IN_PIT_BOX, ph::ON_TRACK) == 0);
    n = t.snapshot(6);
    CHECK(n.serves == 0 && near(n.armed, 10) && n.stop == 0);
    CHECK(t.box_extra(6) == 0);

    t.arm(7, 4);
    t.on_status(7, ph::ON_TRACK, 15);
    CHECK(t.box_extra(7) == 0 && t.snapshot(7).stop == 0);
    t.on_status(7, 15, ph::ON_TRACK);
    CHECK(near(t.snapshot(7).armed, 4));

    t.arm(8, NAN);
    CHECK(t.snapshot(8).armed == 0);
    t.arm(8, -2);
    CHECK(t.snapshot(8).armed == 0);
    t.arm(8, 1e9f);
    CHECK(t.snapshot(8).armed == ph::MAX_SECONDS);

    t.reset_all();
    CHECK(t.snapshot(3).serves == 0 && t.snapshot(8).armed == 0);
}

static void learned_lead()
{
    ph::Table t;
    t.set_crew(ph::CREW_STRETCH);
    t.arm(3, 17);
    enter_box(t, 3);
    CHECK(t.clip_started(3, true, false, 10) == ph::CREW_STRETCH);
    CHECK(near(t.crew_step(3, 0.1f, 1.0f), 0.1f * (10 + ph::STRETCH_LEAD) / 17));
    int steps = 1;
    while (t.car(3).waiting.load() && steps < 1000) {
        t.crew_step(3, 0.1f, 1.0f);
        steps++;
    }
    CHECK(steps > 180 && steps < 200);
    float at_timeout = t.clip_time(3);
    for (int i = 0; i < 4; i++)
        CHECK(near(t.crew_step(3, 0.1f, 1.0f), 0.1f));
    CHECK(!t.crew_start(3));
    float measured = t.clip_time(3);
    CHECK(std::fabs(measured - (at_timeout + 0.4f)) < 1e-3f);
    CHECK(t.lead(10) == measured);
    CHECK(t.lead(5) == 5 + ph::STRETCH_LEAD);
    CHECK(t.crew_step(3, 0.1f, 1.0f) == 0.1f);
    t.on_status(3, ph::IN_PIT_BOX, ph::CREW_COMPLETED);
    t.on_status(3, ph::CREW_COMPLETED, ph::ON_TRACK);

    t.arm(3, 20);
    enter_box(t, 3);
    t.clip_started(3, true, false, 10);
    CHECK(near(t.crew_step(3, 0.1f, 1.0f), 0.1f * measured / 20));
    for (int i = 1; i < 200; i++)
        t.crew_step(3, 0.1f, 1.0f);
    CHECK(std::fabs(t.clip_time(3) - measured) < 1e-2f);
    CHECK(t.crew_start(3));
    CHECK(std::fabs(t.lead(10) - measured) < 1e-2f);

    ph::Table u;
    u.set_crew(ph::CREW_STRETCH);
    u.arm(4, 5);
    enter_box(u, 4);
    u.clip_started(4, true, false, 5);
    u.crew_step(4, 0.1f, 1.0f);
    CHECK(u.crew_start(4));
    CHECK(u.lead(5) == 5 + ph::STRETCH_LEAD);
}

static void stretch()
{
    ph::Table t;
    t.set_crew(ph::CREW_STRETCH);
    t.arm(2, 20);
    CHECK(!t.crew_serves(2));
    t.on_status(2, ph::ON_TRACK, ph::ENTERING_PIT_LANE);
    CHECK(t.crew_serves(2));
    CHECK(!t.crew_serves(1));
    t.on_status(2, ph::ENTERING_PIT_LANE, ph::ENTERING_PIT_BOX);
    t.on_status(2, ph::ENTERING_PIT_BOX, ph::IN_PIT_BOX);
    CHECK(t.clip_started(2, true, false) == ph::CREW_STRETCH);
    float rate = 1.2f;
    CHECK(near(t.crew_step(2, 0.1f * rate, rate), 0.1f * (5 + ph::STRETCH_LEAD) / 20));
    CHECK(near(t.crew_step(2, 0.1f * rate, rate), 0.1f * (5 + ph::STRETCH_LEAD) / 20));
    CHECK(t.crew_start(2));
    CHECK(t.lead(5) == 5 + ph::STRETCH_LEAD);
    CHECK(near(t.crew_step(2, 0.12f, rate), 0.12f));
    CHECK(!t.crew_start(2));
    t.on_status(2, ph::IN_PIT_BOX, ph::PERFORMED_RELEASE);
    CHECK(t.snapshot(2).crew == ph::CREW_STRETCH);
    t.on_status(2, ph::PERFORMED_RELEASE, ph::LEAVING_PIT_BOX);
    t.on_status(2, ph::LEAVING_PIT_BOX, 14);
    CHECK(!t.crew_serves(2));

    t.arm(4, 3);
    enter_box(t, 4);
    t.clip_started(4, true, false);
    CHECK(near(t.crew_step(4, 0.1f, 1.0f), 0.1f * (5 + ph::STRETCH_LEAD) / 3));

    t.arm(12, 15);
    enter_box(t, 12);
    CHECK(t.clip_started(12, true, false, 10) == ph::CREW_STRETCH);
    CHECK(near(t.crew_step(12, 0.3f, 1.5f), 0.2f * (10 + ph::STRETCH_LEAD) / 15));

    t.arm(9, 2);
    enter_box(t, 9);
    t.clip_started(9, true, false);
    for (int i = 0; i < 50; i++)
        t.crew_step(9, 0.1f, 1.0f);
    CHECK(near(t.crew_step(9, 0.1f, 1.0f), 0.1f));

    t.arm(10, 6);
    enter_box(t, 10);
    CHECK(t.clip_started(10, true, true) == ph::CREW_FREEZE);
    CHECK(!t.crew_serves(10));
    CHECK(t.crew_step(10, 0.5f, 1.0f) == 0);

    t.arm(11, 6);
    enter_box(t, 11);
    CHECK(t.clip_started(11, false, false) == ph::CREW_FREEZE);
}

static void freeze()
{
    ph::Table t;
    t.set_crew(ph::CREW_FREEZE);
    t.arm(1, 1.0f);
    enter_box(t, 1);
    CHECK(!t.crew_serves(1));
    CHECK(t.clip_started(1, false, false) == ph::CREW_FREEZE);
    CHECK(t.crew_step(1, 0.4f * 1.2f, 1.2f) == 0);
    CHECK(t.crew_step(1, 0.4f * 1.2f, 1.2f) == 0);
    CHECK(near(t.crew_step(1, 0.4f * 1.2f, 1.2f), 0.2f * 1.2f));
    CHECK(near(t.crew_step(1, 0.4f, 1.2f), 0.4f));
    CHECK(!t.crew_start(1));
    CHECK(t.crew_step(1, 0.1f, 0.0f) == 0.1f);

    t.set_crew(ph::CREW_NONE);
    t.arm(2, 5);
    enter_box(t, 2);
    CHECK(t.clip_started(2, false, false) == ph::CREW_NONE);
    CHECK(t.crew_step(2, 0.1f, 1.0f) == 0.1f);
    CHECK(near(t.box_extra(2), 5));

    t.set_crew(ph::CREW_FREEZE);
    t.arm(3, 0);
    enter_box(t, 3);
    CHECK(t.clip_started(3, false, false) == ph::CREW_NONE);
    CHECK(t.crew_step(3, 0.1f, 1.0f) == 0.1f);
}

static void names()
{
    ph::Crew c = ph::CREW_NONE;
    CHECK(ph::crew_from_name("stretch", c) && c == ph::CREW_STRETCH);
    CHECK(ph::crew_from_name("freeze", c) && c == ph::CREW_FREEZE);
    CHECK(ph::crew_from_name("none", c) && c == ph::CREW_NONE);
    CHECK(!ph::crew_from_name("fast", c) && c == ph::CREW_NONE);
    CHECK(!ph::crew_from_name("", c) && !ph::crew_from_name(nullptr, c));
}

int main()
{
    hold();
    stretch();
    learned_lead();
    freeze();
    names();
    if (g_fail) {
        std::printf("pit hold test: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("pit hold test: ok\n");
    return 0;
}
