#pragma once

#include "core/runtime/car_layout.hpp"

// The gates of the live car code: the car points, cars::live and the car levers run on the builds
// runtime::car_layout::CAR_LIVE names.
namespace swofty_core::hooks::car_proofs {

namespace bp = ::runtime::build_proof;

inline constexpr bp::Gate POINTS{"car points", &::runtime::car_layout::CAR_LIVE};
inline constexpr bp::Gate OPERATIONS{"live car operations", &::runtime::car_layout::CAR_LIVE};
inline constexpr bp::Gate LEVERS{"car levers", &::runtime::car_layout::CAR_LIVE};

// The build's name in the "[cars] live car operations on (<name>)" line (docs/releases/1.9.74.md).
constexpr const char *log_name(bp::Build build)
{
    return build == bp::Build::kb              ? "Steam KB"
           : build == bp::Build::steam_1_0_145 ? "Steam 26566308"
           : build == bp::Build::ea_1_0_145    ? "EA 1.0.145"
                                               : nullptr;
}

constexpr bool every_gated_build_named()
{
    for (const bp::Known &k : bp::KNOWN)
        if (bp::proven_on(::runtime::car_layout::CAR_LIVE, k.build) && !log_name(k.build))
            return false;
    return true;
}

static_assert(every_gated_build_named());
}
