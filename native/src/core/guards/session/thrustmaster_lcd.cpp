#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/session/thrustmaster_lcd_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// Thrustmaster LCD guard (re/db/functions.tsv Game_ThrustmasterLcd_UpdateSectors).
//
// With a Thrustmaster wheel connected, Game_GameModule_Update runs Game_ThrustmasterLcd_UpdateSectors every frame to
// write the sector times and the team name of the sector's fastest car to the wheel's LCD block. For the car in a
// sector it takes the entrant out of the session's entrant list and asks FUN_1423dbcf0 (a thunk to the .data1 body at
// 0x14c970d00) for the entrant's record: the table at manager + 0x760 + 0x268 + kind * 0x10, kind 0 to 2, indexed by
// the entrant's index word. The lookup returns 0 for a kind above 2, a missing table or an index past the table's end,
// and the function reads the team name at record + 0xc8 (0x142fdc596) without a check. A box with 24 entrants on a
// 22-car grid reaches that: the session start faults reading 0xc8 at F1_25.exe+0x2fdc596.
//
// site_Game_ThrustmasterLcd_EntrantRecord is the mov rbp,rax that takes the lookup result. When rax is 0 the guard
// continues at site_Game_ThrustmasterLcd_SectorTail, where the function's own not-found paths go: the team name is
// left as the last frame wrote it and the flags state still updates. A record that was found runs the game's code
// unchanged.
//
// SWOFTY_TMLCD_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (thrustmaster lcd guard)";
constexpr const char *RECORD = "site_Game_ThrustmasterLcd_EntrantRecord";
constexpr const char *TAIL = "site_Game_ThrustmasterLcd_SectorTail";

namespace guards = swofty_core::guards;
namespace tl = runtime::thrustmaster_lcd;

uintptr_t sector_tail = 0;
guards::LogFirst<tl::LOG_FIRST> skipped{"skips"};

void entrant_record(SwoftyContext *c)
{
    if (!tl::record_missing(c->rax))
        return;
    c->rip = sector_tail;
    auto line = skipped.note();
    if (!line)
        return;
    swofty::log("[tmlcd] a sector entrant has no record, so the wheel LCD keeps its last team name%s", line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::thrustmaster_lcd,
                        {
                            .set = MIXIN,
                            .name = "[tmlcd] thrustmaster lcd guard",
                            .env = "SWOFTY_TMLCD_GUARD",
                            .keeps = {{TAIL, &sector_tail}},
                            .needs = {RECORD, TAIL},
                            .mixins = {{RECORD, SWOFTY_MIXIN_SITE, 100, (void *)&entrant_record, nullptr}},
                            .armed = "a sector entrant without a record leaves the LCD team name alone",
                        }};

}
