#include "wine_test.hpp"

SWOFTY_WRAP(FeHome_Handle_Command, uint64_t, FEHomeViewModel *vm, NeStr **cmd) { return orig(vm, cmd) * 10 + 4; }

SWOFTY_ADDON()
