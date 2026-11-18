#pragma once

#include "core/runtime/build_proof.hpp"

#include <cstddef>

// Where the ERS system keeps the car's ERS mode (CarErs) on each build with a witness for it: the KB build reads it at
// +0x1a68 (Car_Powertrain_Step), and Steam 1.0.145 reads it and the ERS Reset stores it at +0x1a6c
// (re/research/live-steam-26566308.md). On any other build mode_offset is 0, and the ERS mode guard there neither reads
// the mode back after the reset nor writes it.
namespace runtime::ers_mode {

struct ModeAt {
    size_t offset;
    build_proof::Proof proof;
};

inline constexpr ModeAt MODE_AT[] = {
    {0x1a68, {"the ERS mode at +0x1a68", build_proof::bit(build_proof::Build::kb)}},
    {0x1a6c, {"the ERS mode at +0x1a6c", build_proof::bit(build_proof::Build::steam_1_0_145)}},
};

constexpr size_t mode_offset(build_proof::Build b)
{
    for (const ModeAt &m : MODE_AT)
        if (build_proof::proven_on(m.proof, b))
            return m.offset;
    return 0;
}

}
