#include "wine_test.hpp"

namespace swofty::game {
using Fake_Stage_Wrap_t = Fake_Stage_t;
}

SWOFTY_WRAP(Fake_Stage_Wrap, int) { return orig() + 80; }

SWOFTY_SITE(site_Stage_Entry, swofty::Context &) { probe()[41] = probe()[41] + 1; }

SWOFTY_ON_LOAD { probe()[44] = probe()[44] + 1; }

SWOFTY_ADDON()
