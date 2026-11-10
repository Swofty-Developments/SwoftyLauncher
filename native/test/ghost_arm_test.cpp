#include "ghost_arm_logic.hpp"

#include <cstdio>
#include <string>

namespace ga = runtime::ghost_arm;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static ga::SlotView slot(bool bound, uint32_t mode, uint32_t count = 0, uint32_t error = 0)
{
    ga::SlotView s;
    s.present = true;
    s.bound = bound;
    s.mode = s.want = mode;
    s.count = count;
    s.error = error;
    return s;
}

static ga::Views session_start()
{
    ga::Views v;
    v.s[0] = slot(true, 0);
    v.s[1] = slot(false, 0);
    v.s[2] = slot(false, 0);
    v.s[3] = slot(true, 0);
    v.s[4] = slot(false, 0);
    v.s[5] = slot(false, 0);
    return v;
}

static ga::Views window()
{
    ga::Views v = session_start();
    for (auto &x : v.s)
        if (x.present)
            x.mode = x.want = 1;
    v.s[0].mode = v.s[0].want = 0;
    return v;
}

static ga::Views first_lap(uint32_t n)
{
    ga::Views v = session_start();
    for (auto &s : v.s)
        s.mode = s.want = 2;
    v.s[0] = slot(true, 3, n);
    v.s[3] = slot(true, 1);
    return v;
}

static ga::Views after_lap()
{
    ga::Views v = first_lap(100);
    v.s[1] = slot(false, 2, 2009);
    v.s[2] = slot(false, 2, 2009);
    v.s[3] = slot(true, 2, 2009);
    return v;
}

int main()
{
    ga::Arming a;
    a.component(0x500f27f60);
    a.request(1);
    expect(a.decide(session_start(), -1, 0).what == ga::Do::none, "nothing at session start (0 bound m0 n0, 3 bound m0 n0)");
    expect(std::string(a.why()).find("settled") != std::string::npos, "the pending reason names the settling");
    auto d = a.decide(window(), -1, 1);
    expect(d.what == ga::Do::arm && d.slot == 3, "arms slot 3 in the window before timing (0 bound m0, 3 bound m1)");
    expect(a.decide(first_lap(12), -1, 1).what == ga::Do::arm, "arms at once mid-lap (n12)");
    d = a.decide(first_lap(60), -1, 2);
    expect(d.what == ga::Do::arm && d.slot == 3, "arms slot 3 when the rival is unbound and the lap is live");
    auto v = first_lap(60);
    v.s[4] = slot(true, 2);
    d = a.decide(v, -1, 3);
    expect(d.what == ga::Do::arm && d.slot == 4, "prefers a bound rival slot");
    d = a.decide(first_lap(60), 4, 3);
    expect(d.what == ga::Do::none, "an unbound env slot never arms");
    d = a.decide(first_lap(60), 3, 3);
    expect(d.what == ga::Do::arm && d.slot == 3, "env slot 3");
    a.armed_ok(3);
    expect(a.decide(after_lap(), -1, 4).what == ga::Do::none, "armed once per component");
    a.request(0);
    d = a.decide(after_lap(), -1, 5);
    expect(d.what == ga::Do::restore && d.slot == 3, "a cleared request restores slot 3");
    a.restored();
    expect(a.decide(after_lap(), -1, 6).what == ga::Do::none, "nothing after the restore");

    ga::Arming b;
    b.component(1);
    b.request(7);
    auto bad = first_lap(80);
    bad.s[3].error = 1;
    expect(b.decide(bad, -1, 0).what == ga::Do::none, "a slot with an error is not armed");
    expect(b.decide(after_lap(), -1, 0).what == ga::Do::arm, "arms from the state after the first lap");
    b.failed(0);
    expect(b.decide(after_lap(), -1, 100).what == ga::Do::none, "waits after a failure");
    expect(b.decide(after_lap(), -1, 2100).what == ga::Do::arm, "retries after the backoff");
    b.failed(2100);
    b.failed(4200);
    expect(b.decide(after_lap(), -1, 99999).what == ga::Do::none, "gives up after three failures");
    b.component(2);
    expect(b.decide(after_lap(), -1, 99999).what == ga::Do::arm, "a new component resets the attempts");
    b.armed_ok(3);
    b.component(3);
    expect(b.decide(first_lap(60), -1, 0).what == ga::Do::arm, "a new session component arms again");
    b.armed_ok(3);
    b.request(8);
    expect(b.decide(first_lap(60), -1, 1).what == ga::Do::arm, "a new request re-arms");

    ga::Arming c;
    c.component(1);
    expect(c.decide(first_lap(60), -1, 0).what == ga::Do::none, "no request, no action");
    ga::Views none;
    c.request(1);
    expect(c.decide(none, -1, 0).what == ga::Do::none, "absent laps never arm");
    if (failed) {
        std::printf("%d failed\n", failed);
        return 1;
    }
    std::printf("all passed\n");
}
