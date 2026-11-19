#include "wine_test.hpp"

SWOFTY_OVERWRITE(50, FeHome_Handle_Command, uint64_t, FEHomeViewModel *, NeStr **) { return 777; }

SWOFTY_ADDON()
