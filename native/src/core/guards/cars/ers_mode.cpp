#include "core/guards/cars/ers_mode_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

// ERS mode guard (re/db/globals.tsv site_Car_ErsGearModifierLoad).
//
// Car_Powertrain_Step reads the car's ERS mode (CarErs +0x1a68 on the KB build, +0x1a6c on Steam 1.0.145, as
// ers_mode_logic.hpp lists them: 0 None, 1 Medium, 2 Overtake, 3 Hotlap) and loads that mode's gear modifier from the
// four 0x40-byte mode blocks at +0x1880. The ERS system's factory leaves the mode and the lap energy state unset. The
// ERS Reset (system vtable +0x18) sets them only when its flags carry both 0x10 and 0x200. A car's create runs every
// system's Reset with all flags. A reset of the car's systems (Car_Systems_Reset_Body) re-runs
// Car_Assets_CreateSystemForPerformanceMode, which also makes a system the car lacks, and then runs the Resets with
// 0x200 only when its arg is 0. A car whose handling file has no EnergyRecoverySystem section can get its ERS system on
// such a reset, and its mode is whatever the allocator left there: ACCESS_VIOLATION at Car_Powertrain_Step+0x5df
// (F1_25.exe+0x46e286f) as the session starts.
//
// At site_Car_ErsGearModifierLoad (r14 = ERS system, rax = mode, rdx = ERS + mode*0x40) a mode outside 0..3 runs the
// system's own Reset with 0x10|0x200, which sets the mode to Medium and fills the lap energy state as the car's create
// does, and the load then reads the Medium block. A car with a mode in range runs the game's code unchanged. The guard
// reads the mode back after the reset, and writes Medium itself when the reset is not callable, only on a build that
// has a witness for the mode's offset (runtime/build_proof.hpp).
//
// SWOFTY_ERS_MODE_GUARD=0 turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (ers mode guard)";
constexpr const char *SITE = "site_Car_ErsGearModifierLoad";
constexpr size_t ERS_CAR = 0x8;
constexpr size_t MODE_STRIDE = 0x40;
constexpr size_t RESET_SLOT = 0x18;
constexpr size_t CAR_VEHICLE_ID = 0xe8;
constexpr uint32_t RESET_FLAGS = 0x210;
constexpr int32_t MODES = 4;
constexpr int32_t MEDIUM = 1;
constexpr int LOG_FIRST = 8;

using Reset = void(void *, uint32_t);

guards::LogFirst<LOG_FIRST> fixed{"fixes"};

bool reset_ers(uintptr_t ers, size_t mode_at)
{
    uintptr_t vt = 0, fn = 0;
    if (!mem::object_like(ers) || !mem::read(ers, vt) || !mem::read(vt + RESET_SLOT, fn) || !mem::in_image(fn))
        return false;
    reinterpret_cast<Reset *>(fn)(reinterpret_cast<void *>(ers), RESET_FLAGS);
    if (!mode_at)
        return true;
    int32_t now = -1;
    return mem::read(ers + mode_at, now) && now >= 0 && now < MODES;
}

void mode_load(SwoftyContext *c)
{
    int32_t mode = static_cast<int32_t>(c->rax);
    if (mode >= 0 && mode < MODES)
        return;
    uintptr_t ers = c->r14;
    size_t mode_at = runtime::ers_mode::mode_offset(runtime::running_build());
    bool reset = reset_ers(ers, mode_at);
    int32_t now = MEDIUM;
    if (mode_at && (!reset || !mem::read(ers + mode_at, now)))
        std::memcpy(reinterpret_cast<void *>(ers + mode_at), &now, sizeof now);
    c->rax = static_cast<uintptr_t>(now);
    c->rdx = ers + static_cast<uintptr_t>(now) * MODE_STRIDE;
    auto line = fixed.note();
    if (!line)
        return;
    uintptr_t car = 0;
    uint8_t id = 0xff;
    if (mem::read(ers + ERS_CAR, car) && car)
        mem::read(car + CAR_VEHICLE_ID, id);
    swofty::log("[ers] car %u: ERS mode %d out of range; set to %d (Medium)%s%s%s", unsigned(id), mode, now,
                reset ? "" : " without the ERS reset, which was not callable",
                mode_at ? ""
                        : "; this build has no witness for the mode's offset, so the guard did not read the mode "
                          "back or write it",
                line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::ers_mode,
                        {
                            .set = MIXIN,
                            .name = "[ers] mode guard",
                            .env = "SWOFTY_ERS_MODE_GUARD",
                            .needs = {SITE},
                            .mixins = {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&mode_load, nullptr}},
                            .armed = "a car whose ERS mode is out of range gets its ERS reset to Medium instead of faulting",
                        }};

}
