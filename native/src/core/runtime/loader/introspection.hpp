#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

// What the rest of the core asks the mixin manager (runtime/loader/mixin_manager.cpp) about the mixins on the game.

// The addon or core set whose handler, or the wrap whose trampoline, holds a code address; empty for none. It takes no
// lock, so the crash report calls it while it names frames.
std::string core_code_owner(uintptr_t addr);

namespace runtime {

// Whether the named core mixin set installed and is live.
bool core_mixins_active(const char *name);
// Whether the last install pass has run and no addon still waits for its code to decrypt.
bool core_mixins_closed();
// The addon whose overwrite mixin sits on a binding or on an address; empty for none.
std::string mixin_overwrite_on(const std::string &binding, uintptr_t at);
// One more than the load position of the addon a module belongs to, or 0 for a module that is no addon.
size_t addon_load_index(uint64_t module);
// What already hooks or patches the five bytes at an address; empty when they are free.
std::string mixin_occupying(uintptr_t at);

}
