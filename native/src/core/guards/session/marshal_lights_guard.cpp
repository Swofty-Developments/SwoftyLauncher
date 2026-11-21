#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/session/marshal_lights_logic.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>

// Marshal lights guard (re/bindings/session/site_Sess_MarshalLights_PrevLightLoad.json,
// site_Sess_MarshalLights_WalkDone.json).
//
// The marshal lights system keeps one record per marshal light entity of the track's world, registered once when the
// world loads, in a list at system + 0x68. On every session enter and every session restart,
// Sess_MarshalLights_AssignToZones walks that list from the back, gives each light the first marshal_light_zone of
// the track markup its lap fraction falls in, and erases a light that is in no zone. After an erase the walk's step
// still loads the light before the erased slot and steps back from there, so the light before an erased one is
// skipped in that pass and is placed by the next pass. When that next pass erases the first light of the list, the
// step loads the 8 bytes before the list's block and stores the lap fraction at +0x34 through them:
// ACCESS_VIOLATION writing 0x34 at F1_25.exe+0x2c794d2 on Steam build 26566308 when they are 0 (when they are not, it
// writes through them and walks on below the list), seen as the session after the first one on the track starts and
// on a session restart.
// A track whose markup has no marshal light zones, such as ashton.lagsec 3.7.0 with 30 lights, erases every other
// light in the first pass and reaches the first light in the second. Suzuka's zones cover the whole lap, so no light
// there is erased.
//
// At site_Sess_MarshalLights_PrevLightLoad, when the slot is at the list's start, the guard sets r13 to it and
// continues at site_Sess_MarshalLights_WalkDone, the walk's own exit when it reaches the list's start, which sorts the
// lights left by zone id and returns. The erased first light stays erased, as the game intends, and nothing is stored.
// Every other step runs the game's code unchanged.
//
// SWOFTY_MARSHAL_LIGHTS_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (marshal lights guard)";
constexpr const char *STEP = "site_Sess_MarshalLights_PrevLightLoad";
constexpr const char *DONE = "site_Sess_MarshalLights_WalkDone";

namespace guards = swofty_core::guards;
namespace ml = runtime::marshal_lights;

uintptr_t step_site = 0;
uintptr_t walk_done = 0;
guards::LogFirst<ml::LOG_FIRST> ended{"skips"};

void prev_light_load(SwoftyContext *c)
{
    if (!ml::walk_ends(c->rsi, c->rbp, guards::read_if_readable, nullptr))
        return;
    c->r13 = c->rbp;
    c->rip = walk_done;
    auto line = ended.note();
    if (!line)
        return;
    uintptr_t base = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
    swofty::log("[guard] +0x%llx null object skipped: the first marshal light is in no marshal light zone, so the "
                "zone walk erases it and ends at the start of the light list instead of reading before it%s",
                (unsigned long long)(step_site + ml::STORE_FROM_SITE - base), line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::marshal_lights,
                        {
                            .set = MIXIN,
                            .name = "[session] marshal lights guard",
                            .env = "SWOFTY_MARSHAL_LIGHTS_GUARD",
                            .keeps = {{STEP, &step_site}, {DONE, &walk_done}},
                            .needs = {STEP, DONE},
                            .mixins = {{STEP, SWOFTY_MIXIN_SITE, 100, (void *)&prev_light_load, nullptr}},
                            .armed = "a track whose first marshal light is in no marshal light zone ends the zone walk "
                                     "at the start of the light list instead of faulting",
                        }};

}
