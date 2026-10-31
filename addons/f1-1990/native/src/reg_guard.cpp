// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the vehicle body registry guard (its own DLL).
//
// Car_Chassis_Build (0x144628190) registers every car's physics body in the registry Game_Init_PhysicsWorldRegistry
// (0x14463adc0) allocates: 0x3BF60 bytes, 26 slots per array, the body count at +0x5d8 incremented with no bound. A
// grid of 26 cars plus the safety car registers 27 bodies, and the 27th registration writes 0 over the first 8 bytes
// of the next heap block's header. The site is the test of the registry pointer (rbx) at 0x14462a65f: when the
// registry already holds 26 bodies the DLL sets rbx to 0 and the following je skips the registration. Option
// registry_guard turns it off.

#include <atomic>
#include <cstdint>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "reg_guard.hpp"

SWOFTY_SETTING(bool, registry_guard, true);

namespace {

constexpr int kLogSkips = 8;

std::atomic<int> g_skipped{0};

}

SWOFTY_SITE(site_Car_ChassisRegistryCount, swofty::Context &c)
{
    if (!registry_guard)
        return;
    auto *registry = reinterpret_cast<const uint8_t *>(c.rbx);
    if (!registry || !swofty::mem::readable(registry, f1990::BodyRegistry::kCount + 4))
        return;
    uint32_t count = f1990::BodyRegistry::count(registry);
    if (!f1990::BodyRegistry::full(count))
        return;
    c.rbx = 0;
    int n = g_skipped.fetch_add(1);
    if (n < kLogSkips)
        swofty::log("[f1-1990] registry guard: body registry %p holds %u bodies, registration #%d skipped (phys %p, "
                    "body %p, %s)",
                    static_cast<const void *>(registry), count, n + 1, reinterpret_cast<void *>(c.r12),
                    reinterpret_cast<void *>(c.rax),
                    f1990::BodyRegistry::scheduled(registry) ? "scheduler registry" : "count-only registry");
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] registry guard: %s", registry_guard ? "on (a 27th body registration is skipped)"
                                                                : "off (registry_guard)");
}

SWOFTY_ADDON()
