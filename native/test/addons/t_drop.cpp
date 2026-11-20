#include "wine_test.hpp"

SWOFTY_WRAP_PRIORITY(100, FeHome_Handle_Command, uint64_t, FEHomeViewModel *, NeStr **) { return 999; }

SWOFTY_WRAP(Fake_Missing, int) { return orig(); }

SWOFTY_ON_LOAD { probe()[4] = 1; }

SWOFTY_ADDON()
