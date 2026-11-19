#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include "core/runtime/pages_logic.hpp"

#include <cstddef>
#include <cstdint>

// Whether the core may read or write memory. Every check shares one protection rule and differs only in how it asks
// Windows and how old its answer may be:
//   runtime::readable (swofty::mem::readable): the page table in mem_pages.cpp, shared by every thread and every
//     module, whose answers are kept for a few milliseconds. Reads of the game's objects use it.
//   pages::readable_now: one VirtualQuery per page and no cache, so it takes no lock and allocates nothing. The crash
//     code reads with it while it handles a fault.
//   pages::readable_in_region: one VirtualQuery for the region at an address, which the n bytes must end inside.
//   loader::page_readable (runtime/loader/image.cpp): the same question behind the binding pass's ProtectionSpans.
// Writes ask writable_in_region (one region, copy-on-write allowed) or read_write_span (runtime::readable, then every
// region the bytes cross must be read-write, copy-on-write refused).
// None of them raises an exception, and none makes a read safe against a page that changes after the answer.
namespace runtime::pages {

// PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY and the three PAGE_EXECUTE_ forms of them.
constexpr DWORD READABLE_ACCESS = 0xEE;
constexpr DWORD WRITABLE_ACCESS = PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
static_assert(WRITABLE_ACCESS == WRITABLE_PROTECTION && PAGE_NOACCESS == NOACCESS_PROTECTION && PAGE_GUARD == GUARD_PROTECTION);

constexpr bool readable_protection(DWORD p) { return (p & READABLE_ACCESS) && !(p & (PAGE_GUARD | PAGE_NOACCESS)); }

inline bool readable_region(const MEMORY_BASIC_INFORMATION &m) { return m.State == MEM_COMMIT && readable_protection(m.Protect); }

inline bool readable_now(uintptr_t a, size_t n)
{
    for (uintptr_t p = a & ~0xfffull; p < a + n; p += 0x1000) {
        MEMORY_BASIC_INFORMATION m;
        if (!VirtualQuery(reinterpret_cast<void *>(p), &m, sizeof m) || !readable_region(m))
            return false;
    }
    return true;
}

inline bool readable_in_region(uintptr_t p, size_t n)
{
    MEMORY_BASIC_INFORMATION m{};
    if (!VirtualQuery(reinterpret_cast<const void *>(p), &m, sizeof m) || !readable_region(m))
        return false;
    return p + n <= reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
}

// n bytes at p sit in one committed region that can be written and is not a guard page.
inline bool writable_in_region(uintptr_t p, size_t n)
{
    MEMORY_BASIC_INFORMATION m{};
    if (VirtualQuery(reinterpret_cast<const void *>(p), &m, sizeof m) != sizeof m)
        return false;
    return writable_answer(m.State == MEM_COMMIT, m.Protect, reinterpret_cast<uintptr_t>(m.BaseAddress), m.RegionSize, p, n);
}

}

namespace runtime {

bool readable(const void *p, size_t n);

}

namespace runtime::pages {

inline bool read_write_span(uintptr_t at, size_t n)
{
    if (!runtime::readable(reinterpret_cast<const void *>(at), n))
        return false;
    uintptr_t p = at, end = at + n;
    while (p < end) {
        MEMORY_BASIC_INFORMATION mbi{};
        if (!VirtualQuery(reinterpret_cast<const void *>(p), &mbi, sizeof mbi) || mbi.State != MEM_COMMIT)
            return false;
        DWORD prot = mbi.Protect & 0xff;
        if (mbi.Protect & (PAGE_GUARD | PAGE_NOACCESS) || !(prot == PAGE_READWRITE || prot == PAGE_EXECUTE_READWRITE))
            return false;
        uintptr_t next = reinterpret_cast<uintptr_t>(mbi.BaseAddress) + mbi.RegionSize;
        if (next <= p)
            return false;
        p = next;
    }
    return true;
}

}
