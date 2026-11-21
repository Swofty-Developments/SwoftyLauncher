#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/cars/tyre_curve_logic.hpp"

#include <swofty/mixin.hpp>

// Tyre curve guard (re/bindings/car/Car_Tyre_CurveMap_FindBracketA.json, Car_Tyre_CurveMap_FindBracketB.json).
//
// The tyre curve sets are trees keyed by a float. Car_Tyre_SampleCurveSet2KeyLerp and Car_Tyre_SampleWearCurve2DAlt
// find the two nodes around each key with Car_Tyre_CurveMap_FindBracketB (outer tree) and Car_Tyre_CurveMap_FindBracketA
// (inner trees). Below the first key or above the last, the search returns that end node twice; otherwise it returns
// the upper bound and the node before it. A key that is not a number fails both range tests (vucomiss; ja) and every
// compare of the upper bound (jb), so the upper bound is the first node and the node before it is the tree's head.
// The sampler then reads the head's value, which holds no curve: ACCESS_VIOLATION at F1_25.exe+0x2e7e5ec (Steam
// 26566308), or +0x2e7ec21 in the second sampler. The keys come from car values, and a car whose values are not
// numbers hands them over right after a wheel is torn off, at a session change, and from the tyre wear objective's bus
// handler.
//
// The guard wraps both searches and passes the lowest float in place of a key that is not a number. The search then
// takes its own below-range exit: both nodes are the first node, and the sampler skips each lerp whose two keys are
// equal. Every other key runs the game's code unchanged.
//
// SWOFTY_TYRE_CURVE_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (tyre curve guard)";
constexpr const char *BRACKET_A = "Car_Tyre_CurveMap_FindBracketA";
constexpr const char *BRACKET_B = "Car_Tyre_CurveMap_FindBracketB";

namespace guards = swofty_core::guards;
namespace tc = runtime::tyre_curve;

using Bracket = void(void *, float, void **);
Bracket *orig_bracket_a = nullptr;
Bracket *orig_bracket_b = nullptr;
guards::LogFirst<tc::LOG_FIRST> replaced{"replacements"};

float checked(float key)
{
    if (!tc::key_unordered(key))
        return key;
    if (auto line = replaced.note())
        swofty::log("[car] a tyre curve lookup got a key that is not a number, so it uses the curve's first point%s", line.tail);
    return tc::lookup_key(key);
}

void bracket_a(void *curves, float key, void **bracket) { orig_bracket_a(curves, checked(key), bracket); }

void bracket_b(void *curves, float key, void **bracket) { orig_bracket_b(curves, checked(key), bracket); }

guards::SiteGuard guard{guards::CoreSetId::tyre_curve,
                        {
                            .set = MIXIN,
                            .name = "[car] tyre curve guard",
                            .env = "SWOFTY_TYRE_CURVE_GUARD",
                            .needs = {BRACKET_A, BRACKET_B},
                            .mixins = {{BRACKET_A, SWOFTY_MIXIN_WRAP, 100, (void *)&bracket_a, (void **)&orig_bracket_a},
                                       {BRACKET_B, SWOFTY_MIXIN_WRAP, 100, (void *)&bracket_b, (void **)&orig_bracket_b}},
                            .armed = "a tyre curve lookup with a key that is not a number uses the curve's first point "
                                     "instead of faulting",
                        }};

}
