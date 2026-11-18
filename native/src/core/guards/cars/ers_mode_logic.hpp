#pragma once

#include "core/runtime/build_proof.hpp"
#include "core/runtime/car_layout.hpp"

#include <cstddef>

// Where the ERS mode guard reads and writes the car's ERS mode (CarErs): at runtime::car_layout::ers_mode, on the builds
// where the guard's reads and the ERS Reset's store of the mode were witnessed: the KB build reads it at +0x1a68
// (Car_Powertrain_Step), and both 1.0.145 builds read it and the ERS Reset stores it at +0x1a6c
// (re/research/live-steam-26566308.md, re/research/ea-1-0-145.md). On any other build mode_offset is 0, and the ERS
// mode guard there neither reads the mode back after the reset nor writes it.
namespace runtime::ers_mode {

namespace bp = build_proof;

inline constexpr bp::Proof RESET_STORE{"the ERS Reset's store of the mode", bp::bit(bp::Build::kb) |
                                                                                bp::bit(bp::Build::steam_1_0_145) |
                                                                                bp::bit(bp::Build::ea_1_0_145)};

constexpr size_t mode_offset(build_proof::Build b)
{
    return build_proof::proven_on(RESET_STORE, b) ? car_layout::ers_mode(b) : 0;
}

static_assert(mode_offset(build_proof::Build::kb) != 0 && mode_offset(build_proof::Build::steam_1_0_145) != 0,
              "every build the guard is proven on has an ERS mode offset");
}
