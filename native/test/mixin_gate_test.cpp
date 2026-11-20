// Host test of the core mixin set gate (core/runtime/loader/mixin_gate.hpp): offer and settle run once each, and the
// commit lines the native-runtime lane's sets log through install_state keep their text.

#include "core/runtime/loader/mixin_gate.hpp"

#include <cstdio>
#include <string>

namespace ld = runtime::loader;

static int failed;

static void expect(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    failed += !ok;
}

static std::string line(const char *format, const char *state)
{
    char buf[512];
    snprintf(buf, sizeof buf, format, state);
    return buf;
}

int main()
{
    ld::MixinGate g;
    expect(!g.offered() && !g.settled(), "a new gate is neither offered nor settled");
    expect(!g.settle() && !g.settled(), "a commit before the offer does not settle");
    expect(g.offer() && g.offered(), "the first offer offers");
    expect(!g.offer() && g.offered(), "a second offer does not offer again");
    expect(g.settle() && g.settled(), "the first commit after the offer settles");
    expect(!g.settle() && g.settled(), "a second commit does not settle again");

    expect(std::string(ld::install_state(true, "armed: x")) == "armed: x", "an installed set gives its armed text");
    expect(std::string(ld::install_state(false, "armed: x")) == "did not install (see the [core] lines)",
           "a set that did not install points at the [core] lines");
    expect(std::string(ld::install_state(false, "armed: x", "off: y")) == "off: y", "a set may give its own off text");

    expect(line("[era] career id aliases %s",
                ld::install_state(true, "armed: a career saved with earlier row ids has its entrants mapped to the "
                                        "current ids as it loads")) ==
               "[era] career id aliases armed: a career saved with earlier row ids has its entrants mapped to the current ids as "
               "it loads",
           "the career alias line");
    expect(line("[era] session vehicle guard %s", ld::install_state(false, "armed: x")) ==
               "[era] session vehicle guard did not install (see the [core] lines)",
           "the session vehicle guard's off line");
    expect(line("[grid] grid rules per box %s",
                ld::install_state(false, "armed: a box's rules.grid re-ranks the weekend's qualifying classification",
                                  "did not install (see the [core] lines); every box keeps the game's grid")) ==
               "[grid] grid rules per box did not install (see the [core] lines); every box keeps the game's grid",
           "the grid rules' off line");
    expect(line("[saves] save splice %s",
                ld::install_state(true, "armed: save containers no loaded game mode claims are kept through each profile "
                                        "load and save")) ==
               "[saves] save splice armed: save containers no loaded game mode claims are kept through each profile load and "
               "save",
           "the save splice line");
    expect(line("[tyres] model check %s",
                ld::install_state(true, "armed: fitted tyre models are read 10 s and 60 s into each session")) ==
               "[tyres] model check armed: fitted tyre models are read 10 s and 60 s into each session",
           "the tyre model check line");

    printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
