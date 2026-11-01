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
    std::printf(failures ? "FAILED\n" : "ALL OK\n");
    return failures ? 1 : 0;
}
