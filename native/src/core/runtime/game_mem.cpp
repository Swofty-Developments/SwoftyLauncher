#include <swofty/mem.hpp>
#include <swofty_api.h>

#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#include "core/diagnostics/perf.hpp"
#include "core/runtime/game_mem_logic.hpp"
#include "core/runtime/loader/bindings.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/loader/resolve_locks.hpp"

// SwoftyCoreGameMem (standard library, swofty/game_mem.hpp): writes into the game's data bindings with the checks
// and the bookkeeping in game_mem_logic.hpp, and allocations through the game's own allocator objects. The owner of
// a call is the addon whose code made it (perf::owner_of on the return address).

namespace runtime::game_mem {

namespace {

using loader::api;

struct State {
    std::mutex lock;
    Ledger ledger;
};

State &state()
{
    static State s;
    return s;
}

template <class... A> void note(const char *fmt, A... a)
{
    if (api && api->log)
        api->log(fmt, a...);
}

// The size bytes at at lie in one section of the game image that is not code.
const char *outside_data(uintptr_t at, size_t size)
{
    auto &img = loader::image();
    if (!img.base())
        return "the game image is not attached yet";
    auto *s = img.section_of(at);
    if (!s || img.section_of(at + size - 1) != s)
        return "the binding's bytes are not inside one section of the game exe";
    if (s->Characteristics & IMAGE_SCN_MEM_EXECUTE)
        return "the binding lies in a code section (code is patched with SWOFTY_PATCH, not written)";
    return nullptr;
}

// Copies n bytes over at with the pages writable for the copy only, then puts their protection back.
bool copy_over(uintptr_t at, const void *from, size_t n)
{
    auto *p = reinterpret_cast<void *>(at);
    DWORD old = 0, back = 0;
    if (!VirtualProtect(p, n, PAGE_READWRITE, &old))
        return false;
    std::memcpy(p, from, n);
    VirtualProtect(p, n, old, &back);
    return true;
}

int write_data_as(const std::string &owner, const char *id, size_t size, size_t offset, const void *bytes, size_t n,
                  const void *expect)
{
    if (!id || !bytes)
        return SWOFTY_ASSET_INVALID;
    uintptr_t base = 0;
    {
        std::lock_guard<std::recursive_mutex> g(loader::resolve_lock());
        loader::Binding *b = loader::bindings().find(id);
        if (!b || b->kind != "data") {
            note("[game-mem] %s: write_data(%s): not a data binding of %s", owner.c_str(), id, loader::bindings().file().c_str());
            return SWOFTY_ASSET_UNAVAILABLE;
        }
    }
    base = loader::resolve_for_addon(id);
    if (!base) {
        note("[game-mem] %s: write_data(%s): the binding does not resolve on this build", owner.c_str(), id);
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    State &st = state();
    std::lock_guard<std::mutex> g(st.lock);
    std::string holder;
    switch (st.ledger.admit(id, size, offset, n, owner, base, &holder)) {
    case Ledger::Admit::ok:
        break;
    case Ledger::Admit::size_mismatch:
        note("[game-mem] %s: write_data(%s): declared size %zu differs from the size earlier writes declared", owner.c_str(), id,
             size);
        return SWOFTY_ASSET_INVALID;
    case Ledger::Admit::out_of_bounds:
        note("[game-mem] %s: write_data(%s): %zu bytes at offset %zu do not fit in its %zu bytes", owner.c_str(), id, n, offset,
             size);
        return SWOFTY_ASSET_INVALID;
    case Ledger::Admit::held:
        note("[game-mem] %s: write_data(%s): addon %s already wrote those bytes", owner.c_str(), id, holder.c_str());
        return SWOFTY_ASSET_INVALID;
    }
    if (const char *why = outside_data(base, size)) {
        note("[game-mem] %s: write_data(%s): %s", owner.c_str(), id, why);
        return SWOFTY_ASSET_INVALID;
    }
    uintptr_t at = base + offset;
    if (!swofty::mem::readable(at, n)) {
        note("[game-mem] %s: write_data(%s): the bytes at %p are not readable", owner.c_str(), id, reinterpret_cast<void *>(at));
        return SWOFTY_ASSET_INVALID;
    }
    std::vector<uint8_t> current(reinterpret_cast<const uint8_t *>(at), reinterpret_cast<const uint8_t *>(at) + n);
    if (expect && std::memcmp(current.data(), expect, n)) {
        note("[game-mem] %s: write_data(%s): the bytes at offset %zu are not the ones the addon expects; left as they "
             "are",
             owner.c_str(), id, offset);
        return SWOFTY_ASSET_INVALID;
    }
    if (!copy_over(at, bytes, n)) {
        note("[game-mem] %s: write_data(%s): VirtualProtect failed (%lu)", owner.c_str(), id, GetLastError());
        return SWOFTY_ASSET_INVALID;
    }
    st.ledger.record(id, size, at, current.data(), n, owner);
    return SWOFTY_ASSET_OK;
}

int restore_data_as(const std::string &owner, const char *id)
{
    State &st = state();
    std::lock_guard<std::mutex> g(st.lock);
    std::vector<Ledger::Write> taken = st.ledger.take(owner, id ? id : ""), failed;
    for (auto &w : taken)
        if (!copy_over(w.at, w.original.data(), w.original.size())) {
            note("[game-mem] %s: restore_data(%s): VirtualProtect failed at %p (%lu); kept for a later restore", owner.c_str(),
                 w.id.c_str(), reinterpret_cast<void *>(w.at), GetLastError());
            failed.push_back(std::move(w));
        }
    bool ok = failed.empty();
    st.ledger.put_back(std::move(failed));
    st.ledger.prune_sizes();
    return ok ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

using AllocFn = void *(*)(void *self, size_t size, size_t align);
using FreeFn = void (*)(void *self, void *p);

// The allocator object, the default one for NULL, when it is a game object whose slot points into the exe image.
void *slot_of(void *allocator, size_t slot, void **fn)
{
    if (!allocator) {
        uintptr_t at = loader::resolve_for_addon("g_Ne_DefaultAllocator");
        if (!at || !swofty::mem::read(at, allocator) || !allocator)
            return nullptr;
    }
    uintptr_t vt = 0, f = 0;
    if (!swofty::mem::object_like(allocator) || !swofty::mem::read(reinterpret_cast<uintptr_t>(allocator), vt) ||
        !swofty::mem::read(vt + slot, f) || !swofty::mem::in_image(f))
        return nullptr;
    *fn = reinterpret_cast<void *>(f);
    return allocator;
}

int api_write_data(const char *id, size_t size, size_t offset, const void *bytes, size_t n, const void *expect)
{
    return write_data_as(perf::owner_of(__builtin_return_address(0)), id, size, offset, bytes, n, expect);
}

int api_restore_data(const char *id) { return restore_data_as(perf::owner_of(__builtin_return_address(0)), id); }

void *api_alloc(void *allocator, size_t size, size_t align)
{
    void *fn = nullptr;
    if (!alloc_args_ok(size, align) || !(allocator = slot_of(allocator, alloc_slot, &fn)))
        return nullptr;
    return reinterpret_cast<AllocFn>(fn)(allocator, size, align);
}

void api_free(void *allocator, void *p)
{
    void *fn = nullptr;
    if (!p || !(allocator = slot_of(allocator, free_slot, &fn)))
        return;
    reinterpret_cast<FreeFn>(fn)(allocator, p);
}

constexpr SwoftyGameMem export_table = {SWOFTY_GAME_MEM_VERSION, api_write_data, api_restore_data, api_alloc, api_free};

}

}

extern "C" __declspec(dllexport) const SwoftyGameMem *SwoftyCoreGameMem(void) { return &runtime::game_mem::export_table; }
