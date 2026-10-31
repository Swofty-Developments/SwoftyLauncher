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
    check("grand prix with a career entity and the box list is the box", f1990::decide_play(false, true) == Play::box);
    check("time trial without a career and the box list is the box", f1990::decide_play(false, true) == Play::box);
    check("stock grand prix is stock", f1990::decide_play(false, false) == Play::stock);
    check("1990 career takes the career path", f1990::decide_play(true, false) == Play::career);
    check("1990 career with the box list still takes the career path", f1990::decide_play(true, true) == Play::career);
    return failures;
}
