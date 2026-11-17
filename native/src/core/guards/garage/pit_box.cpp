#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/garage/pit_box_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <cstring>
#include <vector>

// Pit box guard (re/db/globals.tsv site_Game_PitLane_BoxSlotRead, site_Game_PitLane_BoxPoseFalse).
//
// Game_PitLane_UpdateMovement keeps a car that sits in its garage aligned to its pit box through
// Game_PitLane_ProjectBoxPose(mgr, out, mtx, box, flags), where box is the +0xe0 of the car's pit lane record. The
// function caches one spline handle per box at [mgr + 0x7f8 + box * 8], 40 slots up to the per-car records at
// mgr + 0x938, with no bound on box. A garage holds two cars; a third car (Classic Career seats more than two cars
// to a team) has no box, its +0xe0 is -1, and the read at 0x1434c3656 faults ("reading mgr + 0x800000000 + 0x7f0").
// Garage crew guard (#613) already keeps that car out of the crew spawn; the pit lane then reaches it in the
// hold-in-garage states 10 and 11.
//
// At site_Game_PitLane_BoxSlotRead the guard looks at the box (edi). A box of 40 or more sends the function to its
// shared epilogue with edi = 2, the value its own failed projection leaves there, so it returns false having written
// nothing and the caller leaves the car where it is.
//
// SWOFTY_PIT_BOX_GUARD=0 turns it off.
//
// Race tyre change rule (content.package 1.5 rules.race_tyre_change, apply/content/race_fuel.hpp): a wrap on
// Car_Tyre_ChangeToNextCompound skips the pit stop's tyre change (flag 1) in a race while the pressed box keeps its
// tyres; it returns the 0 the function always returns. Installed always (a pass-through without the rule).
// SWOFTY_RACE_TYRE_RULE=0 turns it off.

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
bool wanted = false;
pb::Seen seen;

constexpr const char *TYRE_MIXIN = "swofty_core.dll (race tyre change rule)";
constexpr const char *TYRE_CHANGE = "Car_Tyre_ChangeToNextCompound";
using ChangeFn = uint64_t(void *tyre_component, char pit_stop);
ChangeFn *orig_change = nullptr;
bool tyre_wanted = false;

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
    pb::Log l = seen.note(box);
    if (l != pb::Log::none)
        swofty::log("[pit] guard: a car in the garage has pit box %d, which has no spline slot; it is not aligned%s",
                    int32_t(box), l == pb::Log::last ? " (further repairs are not logged)" : "");
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
        else {
            tyre_wanted = true;
            out.push_back({TYRE_MIXIN, {{TYRE_CHANGE, SWOFTY_MIXIN_WRAP, 100, (void *)&change_entry, (void **)&orig_change}}});
        }
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
        wanted = true;
        out.push_back({MIXIN, {{READ, SWOFTY_MIXIN_SITE, 100, (void *)&box_read, nullptr}}});
    }

    void settle() override
    {
        if (tyre_wanted)
            swofty::log("[pit] race tyre change rule %s",
                        runtime::core_mixins_active(TYRE_MIXIN)
                            ? "armed: a box with race_tyre_change false keeps its tyres at race stops"
                            : "did not install (see the [core] lines)");
        if (!wanted)
            return;
        swofty::log("[pit] box guard %s",
                    runtime::core_mixins_active(MIXIN) ? "armed: a garage car without a pit box is left where it is"
                                                       : "did not install (see the [core] lines)");
    }
};

PitBoxGuard guard;

}
