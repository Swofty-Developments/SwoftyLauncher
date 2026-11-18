#pragma once

#include "core/runtime/build_proof.hpp"

#include <cstdint>

// Which of the live car's reads a build is allowed. The vehicle table and the CarLive, engine, gearbox, chassis, body
// and steering fields are the std live layouts (live/live_layout.hpp), compared against the EA App build's code
// (docs/reference/std/live.md), and CarLive.suspension, CarSuspension.wheels and CarF1Aero.wing_open_fraction were
// compared with re/tools/layoutcmp.py against the EA dump (same instruction, same displacement), so they read on every
// verified build. The per-wheel block (surface, wheel wetness, spin, slip), the chassis basis, the angular velocity and
// the track wetness read on the builds their proofs below name.

namespace live_car {

struct Fields {
    bool table = false;
    bool wetness = false;
    bool surface = false;
    bool orient = false;
    bool angvel = false;
    bool wheels = false;
    bool drs = false;
};

// On both 1.0.145 builds the wheel spin and slip fields and the suspension's wheel pointers are at the KB displacements,
// and g_Game_WeatherSurfaceState, found there by signature, holds the track wetness at the same +0x3f0a38
// (re/tools/portcheck.py, re/research/port-steam-26566308.md, re/research/ea-1-0-145.md).
namespace bp = ::runtime::build_proof;

inline constexpr bp::Proof WHEEL_FIELDS{"the wheel spin and slip fields (CarSuspension +0x70, CarWheel +0x188, +0x3b0, "
                                        "+0x3b4)",
                                        bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) |
                                            bp::bit(bp::Build::ea_1_0_145)};
inline constexpr bp::Proof TRACK_WETNESS{"the track wetness (g_Game_WeatherSurfaceState +0x3f0a38)",
                                         bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) |
                                             bp::bit(bp::Build::ea_1_0_145)};
// On EA 1.0.145 the vehicle table's car fields (CarLive +0x124, inputs +0x188, +0x190, +0x224, engine, gearbox, chassis,
// steering, suspension and aero pointers, the gear index, engine rates, chassis g and body, body position and velocity,
// steering angle and lock) and the aero's wing_open_fraction +0x19a8 are read at the KB displacements by code that is the
// same there (re/tools/layoutcmp.py, sigport.py --witness; re/research/ea-1-0-145.md).
inline constexpr bp::Proof CAR_TABLE{"the vehicle table's car, engine, gearbox, chassis, body, steering and aero fields",
                                     bp::proofs::VERIFIED_LAYOUT.builds | bp::bit(bp::Build::ea_1_0_145)};
inline constexpr bp::Proof SURFACE_FIELDS{"the wheel surface and wetness, chassis basis and body angular velocity "
                                          "(CarWheel +0x200, +0x280, CarChassis +0x190, body +0x210)",
                                          bp::bit(bp::Build::kb)};

constexpr Fields fields_of(bp::Build build)
{
    Fields f;
    f.table = f.drs = bp::proven_on(CAR_TABLE, build);
    f.wheels = bp::proven_on(WHEEL_FIELDS, build);
    f.wetness = bp::proven_on(TRACK_WETNESS, build);
    f.surface = f.orient = f.angvel = bp::proven_on(SURFACE_FIELDS, build);
    return f;
}

static_assert(!fields_of(bp::Build::unknown).table && !fields_of(bp::Build::unknown).wheels);
static_assert(fields_of(bp::Build::ea).table && fields_of(bp::Build::ea).drs && !fields_of(bp::Build::ea).surface &&
              !fields_of(bp::Build::ea).wetness);
static_assert(fields_of(bp::Build::kb).table && fields_of(bp::Build::kb).surface && fields_of(bp::Build::kb).drs);
static_assert(fields_of(bp::Build::ea_1_0_145).table && fields_of(bp::Build::ea_1_0_145).drs &&
              !fields_of(bp::Build::ea_1_0_145).surface);
static_assert(fields_of(bp::Build::steam_1_0_145).wheels && fields_of(bp::Build::steam_1_0_145).wetness &&
              !fields_of(bp::Build::steam_1_0_145).surface && !fields_of(bp::Build::steam_1_0_145).orient);

}
