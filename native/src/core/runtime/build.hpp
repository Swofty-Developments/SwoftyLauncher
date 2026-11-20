#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

#include "core/runtime/build_proof.hpp"
#include "core/runtime/pages.hpp"

namespace runtime {

// The running game build and how the core finds addresses in it.
// How long a reader of the game's heaps trusts a big region's VirtualQuery answer (swofty::mem::Trust). A fresh
// answer inside one of those multi-GB regions costs milliseconds on Windows. Session start, end and unload drop
// every cached answer (swofty::mem::invalidate).
constexpr uint32_t HEAP_TRUST_MS = 4000;

struct Sig {
    const char *what, *pattern;
    uintptr_t rva;
    enum Kind { addr, rel32, riprel } kind = addr;
    uintptr_t insn = 0, anchor = 0, len = 0;
    const char *section = ".xtls";
};

// The running exe as build_proof.hpp's table knows it.
build_proof::Exe running_exe();
build_proof::Build running_build();
// Whether `gate` holds on the running exe (build_proof.hpp): its proof names this build and its bindings resolve.
// proven() only answers; holds() also logs one [build] line naming what is missing, once per gate.
bool proven(const build_proof::Gate &gate);
bool holds(const build_proof::Gate &gate);
// The KB build: running_build() == Build::kb.
bool same_build();
// Whether build_proof::proofs::VERIFIED_LAYOUT holds: the KB build, its EA App link and Steam 1.0.145. Code that finds every
// address by signature gates on this instead of same_build().
bool verified_build();
// [g_Game_Manager]'s entrants system displacement on the running build (game_manager_layout.hpp): +0x290 on Steam, +0x260 on EA.
uintptr_t manager_entrants_system();

uintptr_t find(const Sig &s, const char *tag, const char *channel = "assets");
// The running build's distribution as the binding set names it: "steam", "ea" or "unknown", and the exe's SHA-256.
std::string build_distribution();
std::string build_exe_sha();

}
