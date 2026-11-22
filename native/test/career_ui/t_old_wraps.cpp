// Test addon for run_career_ui_test.sh: an addon from before the characters and career wizard
// libraries that wraps the functions the core's character and wizard hooks also use, and registers
// nothing with them. Loaded at the window stage with the early stage on, its wraps must install.
#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>

using swofty::game::Action;
using swofty::game::HomeItem;

SWOFTY_WRAP(FeTile_Route_CareerItem, Action *, HomeItem *item, Action *out) { return orig(item, out); }

SWOFTY_WRAP(Game_SpeechSystem_PostLine, void, void *speech, void *line, uint8_t retrigger) { orig(speech, line, retrigger); }

SWOFTY_WRAP(Game_DriverSelectHeader_SetTabState, void, void *header, uint64_t tab, uint8_t visible, uint8_t enabled)
{
    orig(header, tab, visible, enabled);
}

SWOFTY_WRAP(Game_CareerWizard_StartsInF1, bool, void *entrants, void *wizard, const char *group)
{
    return orig(entrants, wizard, group);
}

SWOFTY_ADDON()
