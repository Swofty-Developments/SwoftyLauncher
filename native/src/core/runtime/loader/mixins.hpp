#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "swofty_api.h"

namespace runtime {
struct CoreMixins;
}

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

// The mixin manager (runtime/manager.cpp): addon registration, the session.json manifest check, and the install of
// every addon's and core set's mixins onto the game. Callers hold commit_lock() around the install steps.
Addon *current_addon();
const std::vector<uint8_t> *original_bytes_at(uintptr_t a, uintptr_t &start);
uintptr_t resolve_for_addon(const char *id);
int register_addon_mixins(const SwoftyMixin *m, size_t n);
void read_mixin_manifest(const std::wstring &dir);
int init_addon(const wchar_t *name, SwoftyAddonInitFn init);

void add_core_mixin_sets(std::vector<runtime::CoreMixins> sets);
size_t offer_core_mixin_sets();
void settle_core_mixin_sets();

size_t settled_addons();
size_t active_addons_from(size_t from, size_t *total);
void install_addons(size_t from, size_t *count);
void run_addons_on_load(size_t from);
bool addons_wait(bool core_only);
void reinit_deferred_addons(bool final);
void install_late_addons(size_t *count);
void close_mixins(bool done);

}
