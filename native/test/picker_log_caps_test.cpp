// Host test of the era picker's log caps (core/boxes/picker/log_caps.hpp): each session-load, Time Trial budget and era
// select line is logged for its first loads or presses only, with no tail, and the Time Trial list line keeps its text.

#include "core/boxes/picker/log_caps.hpp"

#include <cstdio>
#include <string>

namespace ep = era_picker;

static int failed;

static void expect(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    failed += !ok;
}

template <class Cap> static int logged(Cap &cap, int calls)
{
    int n = 0;
    for (int i = 0; i < calls; i++)
        if (auto l = cap.note())
            n += std::string(l.tail).empty();
    return n;
}

int main()
{
    static constinit ep::SessionLines s;
    static constinit ep::BudgetLines b;
    static constinit ep::PickerLines p;
    expect(logged(s.career_remap, 20) == 6, "the career id remap line logs 6 times");
    expect(logged(s.front_end_leave, 20) == 6, "the front end leave line logs 6 times");
    expect(logged(s.driver_career, 20) == 6, "the Driver Career load line logs 6 times");
    expect(logged(s.grand_prix, 20) == 6, "the Grand Prix load line logs 6 times");
    expect(logged(s.unload_put_back, 20) == 6, "the unload put-back line logs 6 times");
    expect(logged(s.time_trial_list, 40) == 12, "the Time Trial list line logs 12 times");
    expect(logged(s.time_trial_swap, 40) == 12, "the Time Trial swap line logs 12 times");
    expect(logged(b.empty_override, 20) == 6, "the empty override line logs 6 times");
    expect(logged(b.pool_capped, 20) == 6, "the capped pool line logs 6 times");
    expect(logged(b.budget, 40) == 18, "the budget line logs 18 times");
    expect(logged(p.renarrow, 20) == 6, "the narrowed-again line logs 6 times");
    expect(logged(p.builds, 20) == 6, "the era select build line logs 6 times");
    expect(logged(p.narrow, 20) == 6, "the narrow line logs 6 times");
    expect(logged(p.refused, 20) == 6, "the refused press line logs 6 times");
    expect(logged(p.detached, 20) == 6, "the detached row line logs 6 times");
    expect(s.time_trial_list.count() == 40, "a cap counts the calls past it");

    ep::Cap<ep::LOG_FIRST * 2> list;
    std::string out;
    const char *names[] = {"tt_1", "tt_1", "tt_2"};
    std::string last_seen;
    for (int load = 0; load < 30; load++) {
        std::string line = names[load % 3];
        char buf[256];
        if (line != last_seen && list.note()) {
            snprintf(buf, sizeof buf, "[era] Time Trial session loads list %s (%lld cars) of setup mode %s (box pressed: %s)",
                     line.c_str(), 20LL, "standard", "none");
            out += buf;
            out += "\n";
        }
        last_seen = line;
    }
    size_t lines = 0;
    for (char c : out)
        lines += c == '\n';
    expect(lines == 12, "a list seen again in a row is not counted against the cap");
    expect(out.rfind("[era] Time Trial session loads list tt_1 (20 cars) of setup mode standard (box pressed: none)\n", 0) == 0,
           "the Time Trial list line keeps its text");
    return failed ? 1 : 0;
}
