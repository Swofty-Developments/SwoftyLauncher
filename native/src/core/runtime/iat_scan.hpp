#pragma once
#include <windows.h>

#include "core/runtime/iat_scan_logic.hpp"
#include "core/runtime/imports.hpp"
#include "core/runtime/pages.hpp"

#include <cstdint>

// The import slots of a protected exe, found by what they hold. A packed build keeps no import directory for its
// ws2_32 calls (the directory names one DLL), but its loader fills the same slots in the image before the game runs, so
// each_value_slot visits every qword in the image's data pages that holds `value` and sits in a run of
// iat_scan::MIN_RUN or more pointers into other modules. The slot is the one the game's calls go through, so writing it
// has the effect of the import-table walk in imports.hpp.

namespace runtime {

template <class F> int each_value_slot(void *value, F &&visit)
{
    auto base = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
    auto nt = reinterpret_cast<IMAGE_NT_HEADERS *>(base + reinterpret_cast<IMAGE_DOS_HEADER *>(base)->e_lfanew);
    uintptr_t end = base + nt->OptionalHeader.SizeOfImage;
    int found = 0;
    for (uintptr_t at = base; at < end;) {
        MEMORY_BASIC_INFORMATION m{};
        if (VirtualQuery(reinterpret_cast<void *>(at), &m, sizeof m) != sizeof m)
            break;
        uintptr_t lo = reinterpret_cast<uintptr_t>(m.BaseAddress), hi = lo + m.RegionSize;
        if (lo < base)
            lo = base;
        if (hi > end)
            hi = end;
        at = hi;
        constexpr DWORD EXEC = PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
        if (!pages::readable_region(m) || (m.Protect & EXEC))
            continue;
        auto qw = reinterpret_cast<ULONG_PTR *>(lo);
        size_t n = (hi - lo) / sizeof(ULONG_PTR);
        for (size_t i = 0; i < n; i++) {
            if (qw[i] != reinterpret_cast<ULONG_PTR>(value) ||
                !iat_scan::in_table(reinterpret_cast<const uint64_t *>(qw), n, i, base, end))
                continue;
            ImportSlot s{0, &qw[i], value, reinterpret_cast<uintptr_t>(&qw[i]) - base, {}};
            snprintf(s.how, sizeof s.how, "value");
            visit(s);
            found++;
        }
    }
    return found;
}

}
