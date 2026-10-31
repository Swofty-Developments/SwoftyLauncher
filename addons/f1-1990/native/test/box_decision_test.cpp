// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>

#include "box_decision.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

int main()
{
    using f1990::Play;
    check("list names", f1990::box_list_name("f1_1990_gp") && f1990::box_list_name("f1_1990_tt"));
    check("other lists are not the box",
          !f1990::box_list_name("f1_1990") && !f1990::box_list_name("f1_2026") && !f1990::box_list_name("") &&
              !f1990::box_list_name("f1_1990_gp2"));
    check("grand prix with a career entity and the box list is the box", f1990::decide_play(false, true, true) == Play::box);
    check("time trial without a career and the box list is the box", f1990::decide_play(false, true, true) == Play::box);
    check("stock grand prix is stock", f1990::decide_play(false, false, true) == Play::stock);
    check("1990 career takes the career path", f1990::decide_play(true, false, false) == Play::career);
    check("1990 career with the box list still takes the career path", f1990::decide_play(true, true, true) == Play::career);
    check("stale box list with a 2026 Driver Career setup mode is not the box",
          f1990::decide_play(false, true, f1990::host_mode_name("driver_career_2026")) == Play::stock);
    check("stale box list with a My Team setup mode is not the box",
          f1990::decide_play(false, true, f1990::host_mode_name("my_team")) == Play::stock);
    check("stale box list with the stock f1 setup mode is not the box",
          f1990::decide_play(false, true, f1990::host_mode_name("f1")) == Play::stock);
    check("box list with the swofty_f1 setup mode is the box",
          f1990::decide_play(false, true, f1990::host_mode_name("swofty_f1")) == Play::box);
    return failures;
}
