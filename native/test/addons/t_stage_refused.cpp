#include "wine_test.hpp"

SWOFTY_ON_LOAD { probe()[37] = Fake_Stage_Wrong() + Fake_Stage_Site_Wrong() + Fake_Stage_Ambiguous() + 1; }

SWOFTY_ADDON()
