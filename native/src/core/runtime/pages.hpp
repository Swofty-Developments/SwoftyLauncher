#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>

// Whether the core may read or write memory. Every check shares one protection rule and differs only in how it asks
// Windows and how old its answer may be:
//   runtime::readable (swofty::mem::readable): the page table in mem_pages.cpp, shared by every thread and every
//     module, whose answers are kept for a few milliseconds. Reads of the game's objects use it.
//   pages::readable_now: one VirtualQuery per page and no cache, so it takes no lock and allocates nothing. The crash
//     code reads with it while it handles a fault.
//   loader::page_readable (runtime/loader/image.cpp): one VirtualQuery for the region at an address, which the n bytes
//     must end inside, behind the binding pass's ProtectionSpans.
// None of them raises an exception, and none makes a read safe against a page that changes after the answer.
namespace runtime::pages {

// PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY and the three PAGE_EXECUTE_ forms of them.
constexpr DWORD READABLE_ACCESS = 0xEE;
constexpr DWORD WRITABLE_ACCESS = PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;

constexpr bool readable_protection(DWORD p)
{
    return (p & READABLE_ACCESS) && !(p & (PAGE_GUARD | PAGE_NOACCESS));
}

inline bool readable_region(const MEMORY_BASIC_INFORMATION &m)
{
    return m.State == MEM_COMMIT && readable_protection(m.Protect);
}

inline bool readable_now(uintptr_t a, size_t n)
{
    for (uintptr_t p = a & ~0xfffull; p < a + n; p += 0x1000) {
        MEMORY_BASIC_INFORMATION m;
        if (!VirtualQuery(reinterpret_cast<void *>(p), &m, sizeof m) || !readable_region(m))
            return false;
    }
    return true;
}

// n bytes at p sit in one committed region that can be written and is not a guard page.
inline bool writable_in_region(uintptr_t p, size_t n)
{
    MEMORY_BASIC_INFORMATION m{};
    if (VirtualQuery(reinterpret_cast<const void *>(p), &m, sizeof m) != sizeof m)
        return false;
    if (m.State != MEM_COMMIT || (m.Protect & PAGE_GUARD) || !(m.Protect & WRITABLE_ACCESS))
        return false;
    return p + n <= reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
}

}

namespace runtime {

bool readable(const void *p, size_t n);

}
