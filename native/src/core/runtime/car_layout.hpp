#pragma once

#include "core/runtime/build_proof.hpp"

#include <cstddef>
#include <cstdint>

// The car layouts the core reads, and the builds each is proven on. A car is a CarLive in g_Sess_VehicleTable; it
// caches its systems by section name at +0x530..+0x5d0. The offsets are the KB build's (re/db/structs/Car*.h). A proof
// here names the builds on which the code reading them was compared with the KB build's; a value that moves between
// builds is listed once per build.
namespace runtime::car_layout {

namespace bp = build_proof;

// cars::live (hooks/car_live.hpp): the system slots, vehicle_id +0xe8, is_ai +0x124, the upgrade loader +0x4a0, the
// component owner +0x8, every cars::live field and VTF key, and the functions and mid-hook sites car_live calls and
// hooks. re/tools/carlivecmp.py compared them on both 1.0.145 builds (re/research/car-live-1-0-145.md), and
// re/tools/portcheck.py the hook sites on Steam 1.0.145 (re/research/port-steam-26566308.md).
inline constexpr bp::Proof CAR_LIVE{"CarLive's system slots (+0x530..+0x5d0), +0xe8, +0x124, +0x4a0 and the cars::live "
                                    "fields and hook sites",
                                    bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)};

// The car telemetry fields the test agent reads. Each is read by the same instruction on the KB build and on Steam
// 1.0.145, in a function whose body is the same on both (re/tools/sigport.py --witness, at the KB RVA) or at its readers
// aligned by re/tools/layoutcmp.py: mass +0x150 (0x4676d40); fuel flow and mass +0x100, +0x108 (0x468edb0); max torque
// and limiter +0x2518, +0x2528 (0x4687d60); rotation rate +0x2538 (0x46878d0); throttle +0x2574 (0x46e2290, whose only
// change is the ERS mode); gear +0x680 (0x46a4c20); ERS energy +0x19f0 (0x46d1210); drag +0x19e8 (0x46dceb0); fuel
// capacity +0x98, brake bias +0x21d8 and fuel mode +0x128 at their readers. The system slots, vehicle_id and is_ai they are
// reached through are CAR_LIVE's. On EA 1.0.145 the same functions are the KB's instruction for instruction (the torque
// function apart from the ERS mode and the core's own hook), and fuel capacity, brake bias and fuel mode are CAR_LIVE's
// (re/research/ea-1-0-145.md).
inline constexpr bp::Proof TELEMETRY{"the car telemetry fields (mass +0x150, fuel +0x98, +0x100, +0x108, engine +0x2518, "
                                     "+0x2528, +0x2538, +0x2574, gear +0x680, brake bias +0x21d8, ERS energy +0x19f0, "
                                     "fuel mode +0x128, drag +0x19e8)",
                                     bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)};

// The wheel puncture state the race tyre rule reads (apply/content/race_fuel.hpp): VehicleTyreComponent.physics +0x20,
// CarPhysicsRecord.car +0x58 (0x142e92f70), CarSuspension.wheels +0x70..+0x88 and CarWheel.tyre_flags +0x20, bits 0
// (burst) and 1 (punctured) (0x142e88840, and 0x144656f80, which reaches the byte as wheel+8 +0x18). Each of the three
// is the same function on Steam 1.0.145 and uses each offset at the same instruction (re/tools/sigport.py --witness
// 0x2e88840, 0x2e92f70 and 0x4656f80); a blind static read agreed on both builds. CarLive.suspension +0x580 is CAR_LIVE's.
inline constexpr bp::Proof PUNCTURES{"the wheel puncture state (VehicleTyreComponent +0x20, CarPhysicsRecord +0x58, "
                                     "CarSuspension +0x70..+0x88, CarWheel.tyre_flags +0x20 bits 0 and 1)",
                                     bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};

// A value that differs between builds, with the builds it is proven on.
struct PerBuild {
    size_t value;
    bp::Proof proof;
};

template <size_t N> constexpr size_t on(const PerBuild (&table)[N], bp::Build build)
{
    for (const PerBuild &p : table)
        if (bp::proven_on(p.proof, build))
            return p.value;
    return 0;
}

// CarErs.ers_mode. The KB build reads it at +0x1a68 (Car_Powertrain_Step). On both 1.0.145 builds it and its neighbours
// from +0x1a62 moved by four bytes: the engine torque function reads it and the ERS Reset stores it at +0x1a6c
// (re/research/live-steam-26566308.md, re/research/port-steam-26566308.md, re/research/ea-1-0-145.md).
inline constexpr PerBuild ERS_MODE[] = {
    {0x1a68, {"the ERS mode at +0x1a68", bp::bit(bp::Build::kb)}},
    {0x1a6c, {"the ERS mode at +0x1a6c", bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)}},
};

// g_Game_EngineModeAvail, the one cars::live address that moves: the store of its KB writer 0x1407e5420 (at
// 0x1407e5ec4) aligned on each build (re/research/car-live-1-0-145.md).
inline constexpr PerBuild MODE_AVAIL_RVA[] = {
    {0x606ec28, {"g_Game_EngineModeAvail at 0x606ec28", bp::bit(bp::Build::kb)}},
    {0x6070c28, {"g_Game_EngineModeAvail at 0x6070c28", bp::bit(bp::Build::steam_1_0_145)}},
    {0x61a7968, {"g_Game_EngineModeAvail at 0x61a7968", bp::bit(bp::Build::ea_1_0_145)}},
};

// CarErs.ers_mode's offset on `build`; 0 where it is not proven.
constexpr size_t ers_mode(bp::Build build) { return on(ERS_MODE, build); }

// g_Game_EngineModeAvail's RVA on `build`; 0 where it is not proven.
constexpr size_t mode_avail_rva(bp::Build build) { return on(MODE_AVAIL_RVA, build); }

template <size_t N> constexpr bool one_value_per_build(const PerBuild (&table)[N])
{
    uint32_t seen = 0;
    for (const PerBuild &p : table) {
        if (seen & p.proof.builds)
            return false;
        seen |= p.proof.builds;
    }
    return true;
}

constexpr bool every_car_live_build_has(size_t (*value)(bp::Build))
{
    for (const bp::Known &k : bp::KNOWN)
        if (bp::proven_on(CAR_LIVE, k.build) && !value(k.build))
            return false;
    return true;
}

static_assert((TELEMETRY.builds & ~CAR_LIVE.builds) == 0, "the telemetry fields are read through CAR_LIVE's slots");
static_assert((PUNCTURES.builds & ~CAR_LIVE.builds) == 0, "the puncture state is read through CAR_LIVE's slots");
static_assert(one_value_per_build(ERS_MODE) && one_value_per_build(MODE_AVAIL_RVA));
static_assert(every_car_live_build_has(mode_avail_rva));
static_assert(!ers_mode(bp::Build::unknown) && !mode_avail_rva(bp::Build::unknown));

}
