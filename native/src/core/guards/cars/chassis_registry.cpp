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

// Chassis body registry guard (re/db/globals.tsv site_Car_ChassisRegistryCount).
//
// Car_Chassis_Build registers every car's physics body in the registry Game_Init_PhysicsWorldRegistry allocates:
// 0x3BF60 bytes, 26 slots per array, the body count at +0x5d8 incremented with no bound. A grid of 26 cars plus the
// safety car registers 27 bodies, and the 27th writes 0 over the first 8 bytes of the next heap block's header.
//
// At site_Car_ChassisRegistryCount (the test of the registry pointer in rbx, followed by a je that skips the whole
// registration) the guard sets rbx to 0 once the count has reached 26, so the 27th body is not registered.
//
// SWOFTY_CHASSIS_REGISTRY_GUARD=0 turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;
namespace pb = runtime::pit_box;

constexpr const char *MIXIN = "swofty_core.dll (chassis registry guard)";
constexpr const char *SITE = "site_Car_ChassisRegistryCount";
constexpr uintptr_t COUNT = 0x5d8;
constexpr int LOG_FIRST = 8;

guards::LogFirst<LOG_FIRST> skipped{"skips"};

void registry_count(SwoftyContext *c)
{
    uint32_t count = 0;
    if (!c->rbx || !mem::read(c->rbx + COUNT, count) || !pb::registry_full(count))
        return;
    c->rbx = 0;
    if (auto line = skipped.note())
        swofty::log("[car] chassis registry holds %u bodies; registration #%d is skipped%s", unsigned(count), line.n, line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::chassis_registry,
                        {
                            .set = MIXIN,
                            .name = "[car] chassis registry guard",
                            .env = "SWOFTY_CHASSIS_REGISTRY_GUARD",
                            .needs = {SITE},
                            .mixins = {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&registry_count, nullptr}},
                            .armed = "a 27th car body is not registered",
                        }};

}
