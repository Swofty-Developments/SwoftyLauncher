#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"
#include "core/services/lap_host.hpp"

#include <windows.h>

#include "core/guards/cars/ers_newlap_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

// ERS new-lap guard (re/db/functions.tsv Game_ErsSystem_HandleVehicleNewLap).
//
// The ERS system subscribes Game_ErsSystem_HandleVehicleNewLap to vehicle_newlap, which the per-vehicle
// progress update posts synchronously when a car's lap changes. The handler walks the system's participants
// (self+0x30..+0x38) for the one whose vehicle component (entry+0x28) carries the message's vehicle id at
// +0xe8, then resets that car's lap energy counters. Game_ErsSystem_BindVehicles leaves entry+0x28 at 0 for a
// participant whose entity has no vehicle component, and the ERS update skips those entries, but the new-lap
// handler reads through them: in a Time Trial with such a participant ahead of the player's car, the first
// new lap faults (ACCESS_VIOLATION reading 0xe8 at F1_25.exe+0x25180f7).
//
// site_Game_ErsNewLap_VehicleLookup is the load of entry+0x28 (r9 = the entry). When the entry or its vehicle
// component is null, or the component's vehicle id can't be read, the guard continues at
// site_Game_ErsNewLap_NextVehicle, the handler's own path for an entry that does not match. Every other entry
// runs the game's code unchanged.
//
// The same site fires session.lap: the search stops at the participant whose vehicle id is the message's,
// once per vehicle_newlap, and the core fires the 1.x "lap" event there (a = the car), which the events
// bridge turns into session.lap, before the game's compare. Participants without a vehicle never fire it.
//
// SWOFTY_ERS_GUARD=0 turns it off, and session.lap with it.

namespace {

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (ers new lap guard)";
constexpr const char *LOOKUP = "site_Game_ErsNewLap_VehicleLookup";
constexpr const char *NEXT = "site_Game_ErsNewLap_NextVehicle";
constexpr size_t SELF_BEGIN = 0x30;
constexpr size_t ENTRY_VEHICLE = 0x28;
constexpr size_t VEHICLE_ID = 0xe8;
constexpr int LOG_FIRST = 8;

uintptr_t next_vehicle = 0;
guards::LogFirst<LOG_FIRST> skipped{"skips"};

void lookup_entry(SwoftyContext *c)
{
    uintptr_t vehicle = 0;
    uint8_t id = 0;
    bool known;
    {
        mem::Trust trust(runtime::HEAP_TRUST_MS);
        known = c->r9 && mem::read(c->r9 + ENTRY_VEHICLE, vehicle) && vehicle && mem::read(vehicle + VEHICLE_ID, id);
    }
    uint8_t car = uint8_t(c->rdx & 0xff);
    auto step = runtime::ers::step(c->r9, vehicle, known ? int(id) : -1, car);
    if (step == runtime::ers::Step::lap) {
        runtime::lap_host::lap_line(car);
        runtime::events_fire(SWOFTY_EVENT_LAP, car, 0, nullptr);
    }
    if (step != runtime::ers::Step::skip)
        return;
    c->rip = next_vehicle;
    auto line = skipped.note();
    if (!line)
        return;
    uintptr_t begin = 0;
    mem::read(c->rcx + SELF_BEGIN, begin);
    swofty::log("[ers] vehicle_newlap for vehicle %u: participant %lld of %lld has no vehicle (entry 0x%llx, vehicle "
                "0x%llx); skipped%s",
                unsigned(c->rdx & 0xff), begin ? (long long)(c->rax - begin) / 8 : -1LL,
                begin ? (long long)(c->r8 - begin) / 8 : -1LL, (unsigned long long)c->r9, (unsigned long long)vehicle, line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::ers_newlap,
                        {
                            .set = MIXIN,
                            .name = "[ers] new-lap guard",
                            .env = "SWOFTY_ERS_GUARD",
                            .keeps = {{NEXT, &next_vehicle}},
                            .needs = {LOOKUP, NEXT},
                            .mixins = {{LOOKUP, SWOFTY_MIXIN_SITE, 100, (void *)&lookup_entry, nullptr}},
                            .armed = "a vehicle_newlap reaching a participant without a vehicle skips it instead of faulting",
                        }};

}
