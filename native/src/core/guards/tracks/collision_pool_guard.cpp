#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/tracks/collision_pool_logic.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>

// Collision tile pool (re/bindings/game/site_Game_PhysicsWorldConfigOverrides.json, F1_25.exe+0x445793b on the KB
// build, +0x4459f0b on Steam 26566308).
//
// A track's collision is the TRN~ JPAK of <slot>_common.erp: one VCQTC tile per entry. Ne_MoverCollision_CreateResource
// makes each tile with Ne_MoverCollision_BuildMoverShapes, which takes one object from physics world 1's large-geom
// pool (world +0x40, element 0x220 bytes, a pointer free list). When the pool is empty that tile fails with error 0xc
// and CreateResource stops: every later tile in JPAK order is missing, with no crash and no log line, and cars fall
// through the ground there. The pool is sized once, when Game_Read_PhysicsWorldConfig reads PhysicsWorld.maxLargeGeoms
// (default 2000) and hands the config to Ne_PhysWorld_InitFromConfig. Other large geoms share it, so a track gets about
// 1640 tiles (stock Spa has 1239; a 1663-tile track lost its last tiles in game).
//
// The guard runs at the game's own world-1 overrides (maxRigidBodies 20000, maxSmallGeoms 20000, maxBroadPhaseCollisions
// 60000), after the settings read and after the trigger world copied its config, and raises maxLargeGeoms
// ([rsp + 0x108] = world-1 config + 0xc8) to 8192 objects, never below what the game read. Only the pool count changes:
// the pool is a free list of pointers (no index of a narrower width), the tile list of the resource is a growable
// vector and the tiles hang on one fixed body as a linked list, so nothing else is sized from it. Cost: 0x230 bytes per
// object, 4.4 MiB for 8192 instead of 1.1 MiB. Stock tracks use the same tiles as before.
//
// SWOFTY_COLLISION_GEOM_POOL=<n> sets the pool size (at most 9728, below the 9755-tile eager query-context path no stock
// track takes); 0 or off turns the guard off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (collision tile pool)";
constexpr const char *SITE = "site_Game_PhysicsWorldConfigOverrides";
constexpr const char *ENV = "SWOFTY_COLLISION_GEOM_POOL";

namespace guards = swofty_core::guards;
namespace cp = runtime::collision_pool;

constinit cp::Choice choice;
guards::LogFirst<4> raised{"physics world builds"};

void overrides(SwoftyContext *c)
{
    uint32_t *large = reinterpret_cast<uint32_t *>(c->rsp + cp::kLargeGeomsSlot);
    uint32_t game = *large;
    uint32_t now = cp::raised(game, choice.size);
    *large = now;
    if (auto line = raised.note())
        swofty::log("%s%s", cp::raise_line(game, now, choice.from_env).c_str(), line.tail);
}

class CollisionPool final : public guards::SiteGuard {
public:
    CollisionPool()
        : SiteGuard(guards::CoreSetId::collision_pool,
                    {
                        .set = MIXIN,
                        .name = "[collision] tile pool",
                        .env = "SWOFTY_COLLISION_GEOM_POOL",
                        .needs = {SITE},
                        .mixins = {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&overrides, nullptr}},
                        .armed = "physics world 1 gets a large-geom pool of SWOFTY_COLLISION_GEOM_POOL objects (default "
                                 "8192) instead of the game's 2000, so a track's collision tiles past the first ~1640 load",
                    })
    {
    }

private:
    bool admit() override
    {
        char v[16] = {};
        DWORD n = GetEnvironmentVariableA(ENV, v, sizeof v);
        choice = cp::choose(n && n < sizeof v ? v : nullptr);
        if (n >= sizeof v)
            choice.bad_env = true;
        std::string line = cp::choice_line(choice, ENV);
        if (!line.empty())
            swofty::log("%s", line.c_str());
        return true;
    }
};

CollisionPool guard;

}
