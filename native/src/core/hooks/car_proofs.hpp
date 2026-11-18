#pragma once

#include "core/runtime/build_proof.hpp"

// The builds each part of the live car code is proven on. re/tools/portcheck.py compares a feature's hook sites, functions
// and fields with the KB build's; re/research/port-steam-26566308.md holds the evidence for Steam 1.0.145.
namespace swofty_core::hooks::car_proofs {

namespace bp = ::runtime::build_proof;

inline constexpr bp::Proof HOOK_SITES{"the car hook sites (car built, systems reset, upgrades applied, setup applied, pit "
                                      "lane status) and CarLive +0xe8, +0x4a0",
                                      bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};
inline constexpr bp::Proof OPERATION_FIELDS{"the live car operation fields (CarSteering +0x28, CarDifferential +0xec, "
                                            "+0xf0, CarTurbo +0x2d8, CarChassis +0x98, +0x118, CarFuelEcu +0x28..+0x118)",
                                            bp::bit(bp::Build::kb)};

inline constexpr bp::Gate POINTS{"car points", &HOOK_SITES};
inline constexpr bp::Gate OPERATIONS{"live car operations", &OPERATION_FIELDS};
// The levers' layers reach the car through the live car operations.
inline constexpr bp::Gate LEVERS{"car levers", &OPERATION_FIELDS};

}
