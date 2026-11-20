#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include "core/runtime/loader/mixins.hpp"
#include "safetyhook.hpp"
#include "swofty_api.h"

namespace runtime::loader {

struct Handler {
    size_t addon;
    Mixin *m;
};

// The mixins of one kind on one address, in priority order, as one install pass plans them.
struct Group {
    uint32_t kind;
    uintptr_t addr;
    size_t span;
    std::vector<Handler> handlers;
    void *tramp = nullptr;
    size_t site = SIZE_MAX;
};

// A group that installed, kept so that a later pass chains onto it instead of hooking the address twice.
struct Held {
    uint32_t kind;
    uintptr_t addr;
    size_t span;
    std::vector<Handler> handlers;
    void *tramp;
    size_t site;
};

template <size_t I> void site_thunk(safetyhook::Context &c);

// Every change the mixins made to the game: the inline and mid hooks, the redirect stubs, the bytes written over
// vtable slots and patches, and the handler chain behind each site. A pass that fails rolls back to the end of the
// last pass that installed.
class PatchSet {
public:
    static constexpr size_t MAX_SITES = 256;
    static constexpr size_t SITE_CHAIN_ROOM = 32;

    const char *install(Group &g);
    void rollback();
    void keep();
    bool chain_fits(size_t site, size_t handlers) const;
    void extend_site(size_t site, const std::vector<Handler> &handlers);
    const std::vector<uint8_t> *original_bytes(uintptr_t a, uintptr_t &start) const;
    std::string trampoline_owner(uintptr_t addr) const;
    std::string occupying(uintptr_t at) const;

private:
    struct SiteHandler {
        void (*fn)(SwoftyContext *);
        uint32_t probe;
    };

    struct Mark {
        size_t written, stubs, mids, inlines, sites;
    };

    template <size_t I> friend void site_thunk(safetyhook::Context &c);

    const char *failed(std::string why, const Group &g, bool allocation);
    bool write_bytes(uintptr_t at, const void *data, size_t n);
    void push_site(size_t site, const Handler &h);

    std::vector<safetyhook::InlineHook> inline_hooks_;
    std::vector<std::string> inline_names_;
    std::vector<safetyhook::MidHook> mid_hooks_;
    std::vector<safetyhook::Allocation> stubs_;
    std::vector<std::pair<uintptr_t, std::vector<uint8_t>>> written_;
    std::vector<SiteHandler> site_chains_[MAX_SITES];
    size_t sites_used_ = 0;
    Mark mark_{};
    std::string install_error_;
};

extern PatchSet patches;

}
