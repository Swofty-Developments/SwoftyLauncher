#include "wine_test.hpp"

SWOFTY_WRAP_PRIORITY(10, FeHome_Handle_Command, uint64_t, FEHomeViewModel *vm, NeStr **cmd) { return orig(vm, cmd) * 10 + 2; }

SWOFTY_VTABLE_PRIORITY(10, vt_Fake_Other, 8, uintptr_t) { return orig() * 3; }

SWOFTY_PATCH(site_Fake_Patch, 0xB8, 0x02, 0x00, 0x00, 0x00)

SWOFTY_ADDON()
