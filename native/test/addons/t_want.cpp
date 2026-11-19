#include "wine_test.hpp"

namespace swofty::game {
using Fake_Unlisted_t = int();
inline constexpr fn<"wine-test:Fake_Unlisted", Fake_Unlisted_t> Fake_Unlisted{};
}

SWOFTY_WRAP_PRIORITY(100, FeHome_Handle_Command, uint64_t, FEHomeViewModel *, NeStr **) { return 888; }

SWOFTY_ON_LOAD { probe()[5] = Fake_Missing() + Fake_Unlisted() + 1; }

SWOFTY_ADDON()
