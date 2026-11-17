#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

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

bool same_build();
// Steam build 26566308 (F1 25 1.0.145, sha256 46009bd3...). Code whose struct offsets were checked on it against the KB build
// (re/tools/sigport.py, sigverify.py --disp) gates on same_build() || steam_1_0_145().
bool steam_1_0_145();
// The builds whose struct layouts were checked against the signatures' functions (re/tools/sigverify.py): the Steam KB build and
// the EA App link of the same version. Code that finds every address by signature gates on this instead of same_build().
bool verified_build();
// [g_Game_Manager]'s entrants system displacement on the running build (game_manager_layout.hpp): +0x290 on Steam, +0x260 on EA.
uintptr_t manager_entrants_system();

uintptr_t find(const Sig &s, const char *tag, const char *channel = "assets");
// The running build's distribution as the binding set names it: "steam", "ea" or "unknown", and the exe's SHA-256.
std::string build_distribution();
std::string build_exe_sha();

}
