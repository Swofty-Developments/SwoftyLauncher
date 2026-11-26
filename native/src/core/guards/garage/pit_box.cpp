#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/garage/pit_box_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// Pit box guard (re/db/globals.tsv site_Game_PitLane_BoxSlotRead, site_Game_PitLane_BoxPoseFalse).
//
// Game_PitLane_UpdateMovement keeps a car that sits in its garage aligned to its pit box through
// Game_PitLane_ProjectBoxPose(mgr, out, mtx, box, flags), where box is the +0xe0 of the car's pit lane record. The
// function caches one spline handle per box at [mgr + 0x7f8 + box * 8], with no bound on box. The cache has 24 slots:
// Game_PitLane_GenerateSplines (0x1434ca550) zeroes only +0x7f8..+0x8b7 and keeps tuning parameter handles from
// +0x8b8 ("box_entry_distance_multiplier" and others), and Game_Pitlane_ApplyVehicleResetState (0x1434c03c0) checks
// the box as signed <= 0x17 before its call. A box from 24 to 39 reads a tuning handle as its spline hint, and on a
// null one the miss path writes a spline handle there (0x1434c36a6); the box 99 reads into the per-car records at
// mgr + 0x938. A garage holds two cars; a third car (Classic Career seats more than two cars to a team) has no box,
// its +0xe0 is -1, and the read at 0x1434c3656 faults ("reading mgr + 0x800000000 + 0x7f0"). Garage crew guard
// (#613) already keeps that car out of the crew spawn; the pit lane then reaches it in the hold-in-garage states 10
// and 11 (Game_PitLane_UpdateMovement calls at 0x1434b9199 and 0x1434b92c6, through 0x1434be460).
//
// At site_Game_PitLane_BoxSlotRead the guard looks at the box (edi). A box of 24 or more sends the function to its
// shared epilogue with edi = 2, the value its own failed projection leaves there, so it returns false having written
// nothing and the caller leaves the car where it is.
//
// SWOFTY_PIT_BOX_GUARD=0 turns it off.
//
// Race tyre change rule (content.package 1.5 rules.race_tyre_change, apply/content/race_fuel.hpp): a wrap on
// Car_Tyre_ChangeToNextCompound skips the pit stop's tyre change (flag 1) in a race while the pressed box keeps its
// tyres; it returns the 0 the function always returns. Installed always (a pass-through without the rule).
// SWOFTY_RACE_TYRE_RULE=0 turns it off.

//
// Box 13 redirect (the 13th team of a 26-car grid, a PitBox13 object): Game_PitLane_UpdateMovement and
// Game_PitLane_MoveDownSpline resolve box 12 from the safety car's placement slot (type 7, slot 0), so the 13th
// team's cars stop on the safety car's spot while its crew stands at PitBox13 (type 8, slot 12). A wrap on both
// functions puts PitBox13's placement into the safety car's slot for the call and restores it after.
// SWOFTY_PIT_BOX_13=0 turns it off.

namespace swofty_core::hooks {
bool race_tyres_keep(uintptr_t tyre_component, bool pit_stop_call);
}

namespace {

constexpr const char *MIXIN = "swofty_core.dll (pit box guard)";
constexpr const char *READ = "site_Game_PitLane_BoxSlotRead";
constexpr const char *FALSE_ = "site_Game_PitLane_BoxPoseFalse";
constexpr uintptr_t PROJECTION_FAILED = 2;

namespace guards = swofty_core::guards;
namespace pb = runtime::pit_box;

uintptr_t pose_false = 0;
pb::Seen seen{"repairs"};

constexpr const char *BOX13_MIXIN = "swofty_core.dll (pit box 13)";
constexpr const char *UPDATE_MOVEMENT = "Game_PitLane_UpdateMovement";
constexpr const char *MOVE_DOWN_SPLINE = "Game_PitLane_MoveDownSpline";
using UpdateMovementFn = void(void *mgr, float dt);
using MoveDownSplineFn = void(void *pitlane, uint8_t car, void *teleport_req, bool alt_spline, float dt, bool p6, bool blend,
                              bool p8, bool p9);
UpdateMovementFn *orig_update_movement = nullptr;
MoveDownSplineFn *orig_move_down = nullptr;
thread_local int box13_depth = 0;
std::atomic<bool> box13_logged{false};

bool box13_engage(void *mgr, pb::Redirect &r)
{
    if (!mgr || box13_depth > 0)
        return false;
    auto *m = static_cast<uint8_t *>(mgr);
    if (!swofty::mem::readable(m + pb::PLACEMENT_OFFSET, sizeof(void *)))
        return false;
    auto *placement = *reinterpret_cast<uint8_t **>(m + pb::PLACEMENT_OFFSET);
    if (!placement || !swofty::mem::readable(placement + pb::SAFETY_CAR_SLOT, sizeof(uint64_t)) ||
        !swofty::mem::readable(placement + pb::BOX_13_SLOT, sizeof(uint64_t)))
        return false;
    if (!r.engage(reinterpret_cast<uint64_t *>(placement + pb::SAFETY_CAR_SLOT),
                  reinterpret_cast<uint64_t *>(placement + pb::BOX_13_SLOT)))
        return false;
    box13_depth++;
    if (!box13_logged.exchange(true))
        swofty::log("[pit] box 13: the pit lane resolves box 12 from PitBox13's placement instead of the safety car's spot");
    return true;
}

void update_movement_entry(void *mgr, float dt)
{
    pb::Redirect r;
    bool on = box13_engage(mgr, r);
    orig_update_movement(mgr, dt);
    if (on) {
        r.release();
        box13_depth--;
    }
}

void move_down_entry(void *pitlane, uint8_t car, void *teleport_req, bool alt_spline, float dt, bool p6, bool blend, bool p8,
                     bool p9)
{
    pb::Redirect r;
    bool on = box13_engage(pitlane, r);
    orig_move_down(pitlane, car, teleport_req, alt_spline, dt, p6, blend, p8, p9);
    if (on) {
        r.release();
        box13_depth--;
    }
}

constexpr const char *TYRE_MIXIN = "swofty_core.dll (race tyre change rule)";
constexpr const char *TYRE_CHANGE = "Car_Tyre_ChangeToNextCompound";
using ChangeFn = uint64_t(void *tyre_component, char pit_stop);
ChangeFn *orig_change = nullptr;

uint64_t change_entry(void *tyre_component, char pit_stop)
{
    if (tyre_component && swofty_core::hooks::race_tyres_keep(reinterpret_cast<uintptr_t>(tyre_component), pit_stop != 0))
        return 0;
    return orig_change(tyre_component, pit_stop);
}

void box_read(SwoftyContext *c)
{
    uint32_t box = uint32_t(c->rdi);
    if (pb::box_usable(box))
        return;
    if (auto line = seen.note(box))
        swofty::log("[pit] guard: a car in the garage has pit box %d, which has no spline slot; it is not aligned%s",
                    int32_t(box), line.tail);
    c->rdi = PROJECTION_FAILED;
    c->rip = pose_false;
}

// Two guards in one core set: the race tyre change rule, then the box guard. The rule stays on when the box guard is
// turned off.
class PitBoxGuard final : public guards::Guard {
public:
    PitBoxGuard() : Guard(guards::CoreSetId::pit_box) {}

private:
    void build(std::vector<runtime::CoreMixins> &out) override
    {
        char tv[16] = {};
        if (guards::env_off("SWOFTY_RACE_TYRE_RULE", tv))
            swofty::log("[pit] race tyre change rule off (SWOFTY_RACE_TYRE_RULE=%s)", tv);
        else if (!swofty::api->resolve(TYRE_CHANGE))
            swofty::log("[pit] race tyre change rule off: this build does not resolve %s", TYRE_CHANGE);
        else
            out.push_back({TYRE_MIXIN, {{TYRE_CHANGE, SWOFTY_MIXIN_WRAP, 100, (void *)&change_entry, (void **)&orig_change}}});
        char b13[16] = {};
        if (guards::env_off("SWOFTY_PIT_BOX_13", b13))
            swofty::log("[pit] box 13 redirect off (SWOFTY_PIT_BOX_13=%s)", b13);
        else if (!swofty::api->resolve(UPDATE_MOVEMENT) || !swofty::api->resolve(MOVE_DOWN_SPLINE))
            swofty::log("[pit] box 13 redirect off: this build does not resolve %s and %s", UPDATE_MOVEMENT, MOVE_DOWN_SPLINE);
        else
            out.push_back(
                {BOX13_MIXIN,
                 {{UPDATE_MOVEMENT, SWOFTY_MIXIN_WRAP, 100, (void *)&update_movement_entry, (void **)&orig_update_movement},
                  {MOVE_DOWN_SPLINE, SWOFTY_MIXIN_WRAP, 100, (void *)&move_down_entry, (void **)&orig_move_down}}});
        char v[16] = {};
        if (guards::env_off("SWOFTY_PIT_BOX_GUARD", v)) {
            swofty::log("[pit] box guard off (SWOFTY_PIT_BOX_GUARD=%s)", v);
            return;
        }
        pose_false = swofty::api->resolve(FALSE_);
        if (!pose_false || !swofty::api->resolve(READ)) {
            pose_false = 0;
            swofty::log("[pit] box guard off: needs %s and %s, which this build does not resolve", READ, FALSE_);
            return;
        }
        out.push_back({MIXIN, {{READ, SWOFTY_MIXIN_SITE, 100, (void *)&box_read, nullptr}}});
    }

    void settle() override
    {
        if (offered_set(TYRE_MIXIN))
            swofty::log("[pit] race tyre change rule %s",
                        guards::install_state(runtime::core_mixins_active(TYRE_MIXIN),
                                              "armed: a box with race_tyre_change false keeps its tyres at race stops"));
        if (offered_set(BOX13_MIXIN))
            swofty::log("[pit] box 13 redirect %s",
                        guards::install_state(runtime::core_mixins_active(BOX13_MIXIN),
                                              "armed: box 12 of a grid with a PitBox13 resolves to PitBox13"));
        if (!offered_set(MIXIN))
            return;
        swofty::log("[pit] box guard %s", guards::install_state(runtime::core_mixins_active(MIXIN),
                                                                "armed: a garage car without a pit box is left where it is"));
    }
};

PitBoxGuard guard;

}
