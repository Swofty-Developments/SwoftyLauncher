#include "core/guards/garage/pit_garage_state_logic.hpp"

#include <cstdio>

static int failures;

static void check(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    failures += !ok;
}

using namespace runtime::pit_garage_state;

int main()
{
    check(slot_of(0) == 0 && slot_of(1) == 0 && slot_of(23) == 11, "two boxes share a garage slot");
    check(slot_of(99) == 12, "the box 99 has the slot 12");
    check(slot_of(27) == 13 && slot_usable(slot_of(27)), "the slot 13 the game gives a car past 26 is written");
    check(slot_usable(slot_of(99)) && slot_usable(slot_of(25)), "every slot the game assigns is written");
    check(slot_of(0xffffffffu) == 0x7fffffffu && !slot_usable(slot_of(0xffffffffu)),
          "a car without a garage (box -1) has no slot");
    check(!slot_usable(slot_of(28)) && !slot_usable(slot_of(39)), "a box past the game's garages has no slot");
    check(!slot_usable(0xffffffffffffffffull), "a stray 64-bit slot has none");
    return failures ? 1 : 0;
}
