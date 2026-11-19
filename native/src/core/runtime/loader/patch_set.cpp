#include "core/runtime/loader/patch_set.hpp"

#include <array>
#include <cstring>

#include "core/diagnostics/perf.hpp"
#include "core/runtime/hook_errors.hpp"
#include "core/runtime/loader/bindings.hpp"
#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/hook_pool.hpp"
#include "core/runtime/loader/image.hpp"

namespace runtime::loader {

static_assert(sizeof(SwoftyContext) == sizeof(safetyhook::Context64));
static_assert(offsetof(SwoftyContext, r12) == offsetof(safetyhook::Context64, r12));
static_assert(offsetof(SwoftyContext, rip) == offsetof(safetyhook::Context64, rip));

constinit PatchSet patches;

template <size_t I> void site_thunk(safetyhook::Context &c)
{
    for (auto &h : patches.site_chains_[I]) {
        perf::Scope p(h.probe);
        h.fn(reinterpret_cast<SwoftyContext *>(&c));
    }
}

namespace {

template <size_t... I> constexpr auto make_thunks(std::index_sequence<I...>)
{
    return std::array<safetyhook::MidHookFn, sizeof...(I)>{site_thunk<I>...};
}
constexpr auto site_thunks = make_thunks(std::make_index_sequence<PatchSet::MAX_SITES>{});

void chain(const Group &g, void *last)
{
    for (size_t i = 0; i < g.handlers.size(); i++)
        if (g.handlers[i].m->orig)
            *g.handlers[i].m->orig = i + 1 < g.handlers.size() ? g.handlers[i + 1].m->handler : last;
}

}

const std::vector<uint8_t> *PatchSet::original_bytes(uintptr_t a, uintptr_t &start) const
{
    for (auto &h : inline_hooks_)
        if (a - h.target_address() < h.original_bytes().size())
            return start = h.target_address(), &h.original_bytes();
    for (auto &h : mid_hooks_)
        if (a - h.target_address() < h.original_bytes().size())
            return start = h.target_address(), &h.original_bytes();
    for (auto &w : written_)
        if (a - w.first < w.second.size())
            return start = w.first, &w.second;
    return nullptr;
}

bool PatchSet::write_bytes(uintptr_t at, const void *data, size_t n)
{
    DWORD old;
    auto p = reinterpret_cast<void *>(at);
    if (!VirtualProtect(p, n, image().in_exec(at) ? PAGE_EXECUTE_READWRITE : PAGE_READWRITE, &old))
        return false;
    auto b = reinterpret_cast<const uint8_t *>(p);
    written_.push_back({at, std::vector<uint8_t>(b, b + n)});
    memcpy(p, data, n);
    VirtualProtect(p, n, old, &old);
    FlushInstructionCache(GetCurrentProcess(), p, n);
    return true;
}

void PatchSet::rollback()
{
    for (size_t i = written_.size(); i-- > mark_.written;) {
        auto &w = written_[i];
        DWORD old;
        auto p = reinterpret_cast<void *>(w.first);
        VirtualProtect(p, w.second.size(), PAGE_EXECUTE_READWRITE, &old);
        memcpy(p, w.second.data(), w.second.size());
        VirtualProtect(p, w.second.size(), old, &old);
        FlushInstructionCache(GetCurrentProcess(), p, w.second.size());
    }
    written_.resize(mark_.written);
    stubs_.erase(stubs_.begin() + mark_.stubs, stubs_.end());
    mid_hooks_.erase(mid_hooks_.begin() + mark_.mids, mid_hooks_.end());
    inline_hooks_.erase(inline_hooks_.begin() + mark_.inlines, inline_hooks_.end());
    inline_names_.erase(inline_names_.begin() + mark_.inlines, inline_names_.end());
    for (size_t i = mark_.sites; i < MAX_SITES; i++)
        site_chains_[i].clear();
    sites_used_ = mark_.sites;
}

void PatchSet::keep() { mark_ = {written_.size(), stubs_.size(), mid_hooks_.size(), inline_hooks_.size(), sites_used_}; }

bool PatchSet::chain_fits(size_t site, size_t handlers) const
{
    return site < MAX_SITES && handlers <= site_chains_[site].capacity();
}

void PatchSet::push_site(size_t site, const Handler &h)
{
    site_chains_[site].push_back({reinterpret_cast<void (*)(SwoftyContext *)>(h.m->handler),
                                  perf::probe(perf::owner_of(h.m->handler), "site " + h.m->target)});
}

void PatchSet::extend_site(size_t site, const std::vector<Handler> &handlers)
{
    for (auto &x : handlers)
        push_site(site, x);
}

const char *PatchSet::failed(std::string why, const Group &g, bool allocation)
{
    install_error_ = std::move(why) + "; bytes at the target: " + target_bytes(g.addr);
    if (allocation)
        hook_pool().dump_free_regions_near(g.addr);
    return install_error_.c_str();
}

const char *PatchSet::install(Group &g)
{
    void *first = g.handlers[0].m->handler;
    if (code().packed() && g.kind != SWOFTY_MIXIN_VTABLE && code().page_encrypted(g.addr))
        return "the code there still reads encrypted";
    switch (g.kind) {
    case SWOFTY_MIXIN_WRAP: {
        auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(g.addr), first, safetyhook::InlineHook::StartDisabled);
        if (!h)
            return failed(runtime::hook_error(h.error()), g, h.error().type == safetyhook::InlineHook::Error::BAD_ALLOCATION);
        g.tramp = h->original<void *>();
        chain(g, g.tramp);
        if (auto on = h->enable(); !on)
            return failed("enabling it failed: " + runtime::hook_error(on.error()), g, false);
        inline_hooks_.push_back(std::move(*h));
        inline_names_.push_back(g.handlers[0].m->target);
        return nullptr;
    }
    case SWOFTY_MIXIN_SITE: {
        if (sites_used_ == MAX_SITES)
            return "too many sites";
        size_t s = sites_used_++;
        g.site = s;
        site_chains_[s].reserve(SITE_CHAIN_ROOM);
        for (auto &h : g.handlers)
            push_site(s, h);
        auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(g.addr), site_thunks[s]);
        if (!h) {
            auto &e = h.error();
            bool alloc = e.type == safetyhook::MidHook::Error::BAD_ALLOCATION ||
                         e.inline_hook_error.type == safetyhook::InlineHook::Error::BAD_ALLOCATION;
            return failed(runtime::hook_error(e), g, alloc);
        }
        mid_hooks_.push_back(std::move(*h));
        return nullptr;
    }
    case SWOFTY_MIXIN_VTABLE:
        chain(g, *reinterpret_cast<void **>(g.addr));
        return write_bytes(g.addr, &first, sizeof first) ? nullptr : "VirtualProtect failed";
    case SWOFTY_MIXIN_REDIRECT: {
        auto stub = safetyhook::Allocator::global()->allocate_near({reinterpret_cast<uint8_t *>(g.addr)}, 14);
        if (!stub) {
            install_error_ = std::string("the jump stub did not allocate: ") + alloc_error(stub.error());
            hook_pool().dump_free_regions_near(g.addr);
            return install_error_.c_str();
        }
        uint8_t jmp[14] = {0xFF, 0x25};
        memcpy(jmp + 6, &first, 8);
        memcpy(stub->data(), jmp, sizeof jmp);
        chain(g, reinterpret_cast<void *>(g.addr + 5 + *reinterpret_cast<const int32_t *>(g.addr + 1)));
        int32_t rel = (int32_t)(stub->address() - (g.addr + 5));
        stubs_.push_back(std::move(*stub));
        return write_bytes(g.addr + 1, &rel, 4) ? nullptr : "VirtualProtect failed";
    }
    case SWOFTY_MIXIN_PATCH: {
        auto *b = bindings().find(g.handlers[0].m->target);
        if (memcmp(reinterpret_cast<const void *>(g.addr), b->expect.data(), b->expect.size()))
            return "bytes changed since resolve";
        return write_bytes(g.addr, first, g.span) ? nullptr : "VirtualProtect failed";
    }
    }
    return "unknown kind";
}

std::string PatchSet::trampoline_owner(uintptr_t addr) const
{
    for (size_t i = 0; i < inline_hooks_.size(); i++)
        if (addr - inline_hooks_[i].trampoline().address() < inline_hooks_[i].trampoline().size())
            return "trampoline of the wrap on " + inline_names_[i];
    return {};
}

std::string PatchSet::occupying(uintptr_t at) const
{
    for (size_t i = 0; i < inline_hooks_.size(); i++)
        if (inline_hooks_[i].target_address() == at)
            return "the wrap on " + inline_names_[i];
    for (auto &m : mid_hooks_)
        if (m.target_address() >= at && m.target_address() < at + 5)
            return "a mid-hook";
    for (auto &w : written_)
        if (w.first < at + 5 && at < w.first + w.second.size())
            return "patched bytes";
    return {};
}

}
