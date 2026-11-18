#pragma once

#include "core/runtime/build_proof.hpp"
#include "core/runtime/car_layout.hpp"

#include <cstddef>

// Where the ERS mode guard reads and writes the car's ERS mode (CarErs): at runtime::car_layout::ers_mode, on the builds
// where the guard's reads and the ERS Reset's store of the mode were witnessed: the KB build reads it at +0x1a68
// (Car_Powertrain_Step), and Steam 1.0.145 reads it and the ERS Reset stores it at +0x1a6c
// (re/research/live-steam-26566308.md). On any other build mode_offset is 0, and the ERS mode guard there neither reads
// the mode back after the reset nor writes it.
namespace runtime::ers_mode {

inline constexpr build_proof::Proof RESET_STORE{"the ERS Reset's store of the mode",
                                                build_proof::bit(build_proof::Build::kb) |
                                                    build_proof::bit(build_proof::Build::steam_1_0_145)};

constexpr size_t mode_offset(build_proof::Build b)
{
    return build_proof::proven_on(RESET_STORE, b) ? car_layout::ers_mode(b) : 0;
}

static_assert(mode_offset(build_proof::Build::kb) && mode_offset(build_proof::Build::steam_1_0_145),
              "every build the guard is proven on has an ERS mode offset");
}
