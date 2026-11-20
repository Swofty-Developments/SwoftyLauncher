#pragma once

#include <cstdint>

namespace crash::prologue {

// The scan cache's section field for the crash hooks' prologue addresses. It names no section of the exe, so no binding
// scan reads or replaces these entries, and the pattern field holds the prologue in the scan cache's token format.
inline constexpr const char *CACHE_SECTION = "crash:exe";

struct Found {
    uintptr_t at = 0;
    bool cached = false;
};

// The address a prologue had on the last scan of this exe, when check() finds the prologue's bytes still there; else a
// new scan of the exe, whose answer replaces the cached one (a scan that finds no match or several removes it).
template <class Recall, class Check, class Scan, class Remember>
Found find(uintptr_t base, Recall &&recall, Check &&check, Scan &&scan, Remember &&remember)
{
    uint64_t rva = 0;
    if (recall(rva) && check(base + uintptr_t(rva)))
        return {base + uintptr_t(rva), true};
    uintptr_t at = scan();
    remember(at ? 1 : 0, at ? uint64_t(at - base) : uint64_t(0));
    return {at, false};
}

}
