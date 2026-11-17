#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/garage/garage_slots_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <cstring>
#include <vector>

// Garage slot guard (re/db/globals.tsv site_Sess_GarageSlots_GarageLoad, site_Sess_GarageSlots_Done).
//
// Sess_Assign_GarageSlotsByPreference(registry, team_entrants, garage_index, garages, out) reads the team's garage as
// [[garages + 8] + garage_index * 8] with no bound. Sess_Assign_GarageSlotsByTeamOrder passes the team's position in
// the team order list as the index and never compares it with the garage count on that path, so a box with more
// teams than the track world has garages (F1 2010's 12 teams at a 10 garage community world) reads past the garage
// vector and Sess_Set_GarageSlotEntrant faults on the stray pointer (Sess_Assign_GarageSlotsByPreference+0x471).
//
// At site_Sess_GarageSlots_GarageLoad (r12 = garages, [rsp + 0x58] = the garage index) the guard compares the index
// with the vector's length. A team past it leaves the function through its own early exit, so the team gets no
// garage slot, the same as a team the game's unlisted-team loop finds no free garage for. Its cars still race; the
// crew and pit box guards already handle a car without a garage.
//
// SWOFTY_GARAGE_SLOT_GUARD=0 turns it off.

namespace {

namespace mem = swofty::mem;
namespace gs = runtime::garage_slots;

constexpr const char *MIXIN = "swofty_core.dll (garage slot guard)";
constexpr const char *LOAD = "site_Sess_GarageSlots_GarageLoad";
constexpr const char *DONE = "site_Sess_GarageSlots_Done";

uintptr_t done = 0;
gs::Seen seen;

void garage_load(SwoftyContext *c)
{
    uint64_t index = 0, begin = 0, end = 0, count = 0;
    bool known = mem::read(c->rsp + gs::FRAME_INDEX, index) && gs::user_pointer(c->r12) &&
                 mem::read(c->r12 + gs::VEC_BEGIN, begin) && mem::read(c->r12 + gs::VEC_END, end) &&
                 gs::count_of(begin, end, &count);
    if (known && gs::has_garage(index, count))
        return;
    if (!known)
        count = 0;
    if (seen.first(count))
        swofty::log("[garage] slots: a team at position %llu of the team order has no garage, the world has %llu; "
                    "teams past the garage count get no garage slot, their cars still race",
                    (unsigned long long)index, (unsigned long long)count);
    c->rip = done;
}

namespace guards = swofty_core::guards;

guards::SiteGuard guard{guards::CoreSetId::garage_slots,
                        {
                            .set = MIXIN,
                            .name = "[garage] slot guard",
                            .env = "SWOFTY_GARAGE_SLOT_GUARD",
                            .keeps = {{DONE, &done}},
                            .needs = {LOAD, DONE},
                            .mixins = {{LOAD, SWOFTY_MIXIN_SITE, 100, (void *)&garage_load, nullptr}},
                            .armed = "a team past the world's garage count gets no garage slot",
                        }};

}
