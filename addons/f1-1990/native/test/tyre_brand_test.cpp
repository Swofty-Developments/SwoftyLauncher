// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "tyre_brand.hpp"

using namespace f1990::brand;

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

int main()
{
    check("Harder is Goodyear", for_preset(kHarder) == Brand::goodyear);
    check("Softer is Pirelli", for_preset(kSofter) == Brand::pirelli);
    check("Balanced keeps the team's supplier", for_preset(kBalanced) == Brand::team && !group(Brand::team));
    check("an unknown preset keeps the team's supplier", for_preset(3) == Brand::team);

    char buf[16] = "F1_1990";
    check("Goodyear group to Pirelli", write_group(buf, sizeof buf, kPirelliGroup) == 1 && !std::strcmp(buf, "F1_1990P"));
    check("already Pirelli", write_group(buf, sizeof buf, kPirelliGroup) == 0);
    check("Pirelli group back to Goodyear", write_group(buf, sizeof buf, kGoodyearGroup) == 1 && !std::strcmp(buf, "F1_1990"));

    char small[8] = "F1_1990";
    check("no room for F1_1990P", write_group(small, sizeof small, kPirelliGroup) == -1 && !std::strcmp(small, "F1_1990"));

    char other[16] = "F1_2026";
    check("a stock group is never touched", write_group(other, sizeof other, kPirelliGroup) == -1 &&
                                                 !std::strcmp(other, "F1_2026"));
    char longer[16] = "F1_1990PX";
    check("a longer name is not a 1990 group", !is_1990_group(longer, sizeof longer));
    char unterminated[8] = {'F', '1', '_', '1', '9', '9', '0', 'P'};
    check("an unterminated buffer is not a 1990 group", !is_1990_group(unterminated, sizeof unterminated));
    check("no wanted group changes nothing", write_group(buf, sizeof buf, nullptr) == -1);

    if (failures)
        std::printf("%d failure(s)\n", failures);
    else
        std::printf("ALL PASSED (0 failure(s))\n");
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
