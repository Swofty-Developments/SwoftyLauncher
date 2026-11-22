#include "wine_test.hpp"

SWOFTY_WRAP(FeHome_Handle_Command, uint64_t, FEHomeViewModel *vm, NeStr **cmd) { return orig(vm, cmd) * 10 + 3; }

SWOFTY_SITE_PRIORITY(-10, site_FeHome_AfterCareerCall, swofty::Context &c)
{
    probe()[1] = c.r12;
    probe()[6] = probe()[6] + 1;
}

SWOFTY_VTABLE(vt_Fake_Other, 8, uintptr_t) { return orig() + 5; }

SWOFTY_REDIRECT(call_Fake_Callee, int) { return orig() + 100; }

SWOFTY_ON_LOAD
{
    static int a, b;
    int *fa = swofty::fields<int>(&a), *fb = swofty::fields<int>(&b);
    *fa = 42;
    bool ok = fa && fb && fa != fb && swofty::fields<int>(&a) == fa && *fb == 0;
    swofty::release_fields(&a);
    probe()[0] = ok && *swofty::fields<int>(&a) == 0;
}

SWOFTY_ADDON()
