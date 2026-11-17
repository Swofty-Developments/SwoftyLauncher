#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "swofty_api.h"

namespace runtime::loader {

struct Mixin {
    std::string target;
    uint32_t kind;
    int32_t priority;
    void *handler;
    void **orig;
    uintptr_t addr = 0;
};

// One mixin owner: an addon DLL loaded through SwoftyCoreInit, or one of the core's own mixin sets.
struct Addon {
    std::string name;
    std::vector<Mixin> mixins;
    bool active = true;
    bool core = false;
    bool waiting = false, late = false, hit_deferred = false;
    SwoftyAddonInitFn reinit = nullptr;
    HMODULE module = nullptr;
};

// The addon whose SwoftyAddonInit is running, or null outside one (mixin_manager.cpp).
Addon *current_addon();
// The game's own bytes under a hook or patch the mixins put on an address, and where that hook or patch starts.
const std::vector<uint8_t> *original_bytes_at(uintptr_t a, uintptr_t &start);
// SwoftyApi::resolve and SwoftyApi::register_mixins for addons.
uintptr_t resolve_for_addon(const char *id);
int register_addon_mixins(const SwoftyMixin *m, size_t n);

}
