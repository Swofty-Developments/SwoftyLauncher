#include <algorithm>
#include <cstdio>

#include "section_schedule.hpp"

static int failures = 0;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++failures; } } while (0)

int main()
{
    using namespace ce::sched;
    CHECK(section_interval_ms(0.5, 0) == 1000);
    CHECK(section_interval_ms(13, 0) == 1300);
    CHECK(section_interval_ms(13, 2) == 5200);
    CHECK(section_interval_ms(13, 9) == 10400);
    CHECK(section_interval_ms(3000, 0) == kMaxMs);
    CHECK(global_gap_ms(13) == 1300);
    CHECK(global_gap_ms(100000) == kMaxMs);

    Slot a, b;
    Clock c;
    CHECK(may_render(a, c, 0) && may_render(b, c, 0));
    rendered(a, c, 0, 13, true);
    CHECK(c.global_ms == 0);
    CHECK(may_render(b, c, 8));
    rendered(b, c, 8, 13, true);
    CHECK(!may_render(a, c, 999));
    CHECK(may_render(a, c, 1400));
    rendered(a, c, 1400, 13, false);
    CHECK(a.unchanged == 1 && a.due_ms == 1400 + 2600);
    rendered(a, c, 5000, 13, true);
    CHECK(a.unchanged == 0 && a.due_ms == 5000 + 1300);
    invalidate_all(&a, 1, c);
    CHECK(may_render(a, c, 0));

    Slot s;
    Clock cl;
    double t = 0, spent = 0;
    int renders = 0;
    for (; t < 60000; t += 16)
        if (may_render(s, cl, t)) {
            rendered(s, cl, t, 13, false);
            spent += 13;
            ++renders;
        }
    CHECK(renders <= 10);
    CHECK(spent / 60000 < 0.0035);

    {
        const double cost[8] = {141, 40, 40, 31, 35, 31, 5, 20};
        const int drivers = 6;
        double worst[2] = {0, 0};
        for (int urgent = 0; urgent < 2; ++urgent)
            for (double offset = 0; offset < 30000; offset += 112) {
                Slot slots[8];
                Clock gc;
                double asked = 60000 + offset, waited = -1;
                for (double now = 0; now < 600000 && waited < 0; now += 16) {
                    bool ask = now >= asked && asked > 0 && slots[drivers].due_ms != -1;
                    if (ask && slots[drivers].unchanged != -7) {
                        slots[drivers].due_ms = 0;
                        slots[drivers].unchanged = -7;
                        if (urgent)
                            slots[drivers].urgent = true;
                        gc.global_ms = 0;
                    }
                    for (int i = 0; i < 8; ++i)
                        if (may_render(slots[i], gc, now)) {
                            bool answered = ask && i == drivers;
                            rendered(slots[i], gc, now, cost[i], answered ? true : (slots[i].unchanged % 2 == 0));
                            if (answered)
                                waited = now - asked;
                            break;
                        }
                }
                worst[urgent] = std::max(worst[urgent], waited < 0 ? 1e9 : waited);
            }
        std::printf("worst wait for the invalidated drivers slot: %.0f ms, urgent: %.0f ms\n", worst[0], worst[1]);
        CHECK(worst[1] <= 100);
    }
    std::printf(failures ? "FAILED\n" : "ALL OK\n");
    return failures ? 1 : 0;
}
