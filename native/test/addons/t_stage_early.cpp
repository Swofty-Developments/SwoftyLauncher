#include "wine_test.hpp"

SWOFTY_SITE(site_Stage_Entry, swofty::Context &) { probe()[32] = probe()[32] + 1; }

SWOFTY_WRAP(Fake_Stage_Wrap_Early, int) { return orig() * 10 + 1; }

SWOFTY_ADDON()
