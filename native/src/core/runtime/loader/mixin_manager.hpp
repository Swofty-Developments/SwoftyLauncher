#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "core/runtime/json.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/loader/patch_set.hpp"
#include "swofty_api.h"

namespace runtime {
struct CoreMixins;
}

namespace runtime::loader {

// The mixin owners and their mixins: addon registration and the session.json manifest check, the core's own mixin
// sets, and each install pass that plans every owner's mixins, drops the owners that conflict or fail, and hands
// the rest to the patch set. Callers hold commit_lock() around the install steps.
class MixinManager {
public:
    int init_addon(const wchar_t *name, SwoftyAddonInitFn init);
    int register_mixins(const SwoftyMixin *m, size_t n);
    uintptr_t resolve(const char *id);
    void read_manifest(const std::wstring &dir);
    Addon *current() const { return current_; }

    void add_core_sets(std::vector<runtime::CoreMixins> sets);
    size_t offer_core_sets();
    void settle_core_sets();

    size_t settled() const { return settled_; }
    size_t active_from(size_t from, size_t *total) const;
    void install(size_t from, size_t *count);
    void run_on_load_from(size_t from);
    bool waiting(bool core_only) const;
    void reinit_deferred(bool final);
    void install_late(size_t *count);
    void close(bool done) { closed_ = done; }

    bool core_set_active(const char *name) const;
    bool closed() const { return closed_; }
    std::string code_owner(uintptr_t addr) const;
    std::string overwrite_on(const std::string &binding, uintptr_t at) const;
    size_t load_index(uint64_t module) const;
    std::string occupying(uintptr_t at) const;

private:
    bool declared(const Addon &addon, const SwoftyMixin &m) const;
    void drop(size_t i, const char *fmt, const char *a = "", const char *b = "", const char *c = "");
    void check_targets(size_t i);
    std::vector<Group> plan(size_t from, bool late = false);
    std::string owners(const Group &g) const;
    bool core_only(const Group &g) const;
    Held *held_for(const Group &g);
    bool drop_conflicts(const std::vector<Group> &groups, bool warn);
    bool install_all(std::vector<Group> &groups);
    void extend_held(Held &h, const Group &g);
    void install_groups(size_t from, bool late, size_t *count);
    void note_waiting(size_t i) const;
    void run_on_load(size_t i);

    std::vector<Addon> addons_;
    Addon *current_ = nullptr;
    size_t settled_ = 0;
    std::vector<Held> held_;
    bool closed_ = false;
    json::Value session_;
    bool have_session_ = false;
};

MixinManager &mixins();

// A mixin kind's name in the loader's lines: wrap, site, vtable, redirect, patch or on_load ("?" for another).
const char *kind_name(uint32_t k);

}
