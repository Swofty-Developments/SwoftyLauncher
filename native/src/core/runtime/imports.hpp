#pragma once
#include <windows.h>

#include "pe_imports.h"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstdio>

// Walks the exe's import slots for one DLL. The game imports some DLLs through more than one
// descriptor (ws2_32 twice: the clear code's table and the one the .data1 bodies call through, the
// second by ordinal), so every descriptor naming the DLL is visited; `table` counts them from 1 in
// directory order. `fn` is what the import resolves to in `lib`, so a slot is matched by what it
// imports, not by what it holds now: a slot another hook already took still matches. The walk is
// pe_imports.h's: an entry another module left pointing outside the image is skipped and logged once.

namespace runtime {

struct ImportSlot {
    int table;
    ULONG_PTR *slot;
    void *fn;
    uintptr_t rva;
    char how[24];
};

inline int import_readable(uintptr_t at, size_t n, void *)
{
    return swofty::mem::readable(at, n);
}

inline bool write_slot(ULONG_PTR *slot, void *value)
{
    DWORD old;
    if (!VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old))
        return false;
    *slot = reinterpret_cast<ULONG_PTR>(value);
    VirtualProtect(slot, sizeof *slot, old, &old);
    return true;
}

inline int import_write(uint64_t *slot, const void *value, void *)
{
    return write_slot(reinterpret_cast<ULONG_PTR *>(slot), const_cast<void *>(value));
}

inline void log_import_skip(std::atomic<bool> &once, const char *tag, const char *module, const pe_import &e,
                            int skipped)
{
    if (once.exchange(true, std::memory_order_relaxed))
        return;
    swofty::log("%s skipped corrupt import entry in %s: table %d (%s) entry %llu, %s (0x%llx at +0x%llx); %d "
                "entr%s skipped there, later skips not logged",
                tag, module, e.table, e.dll ? e.dll : "?", static_cast<unsigned long long>(e.index), e.why,
                static_cast<unsigned long long>(e.value), static_cast<unsigned long long>(e.value_rva), skipped,
                skipped == 1 ? "y" : "ies");
}

inline std::atomic<bool> each_import_skip_logged{false};

template <class F> int each_import(HMODULE lib, const char *dll, F &&visit)
{
    pe_imports it;
    pe_import e, first{};
    if (!pe_imports_open(&it, reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr)), dll, import_readable, nullptr))
        return 0;
    int skipped = 0;
    for (int r; (r = pe_imports_next(&it, &e)) != PE_IMPORT_END;) {
        if (r == PE_IMPORT_SKIP) {
            if (!skipped++)
                first = e;
            continue;
        }
        ImportSlot s{e.table, reinterpret_cast<ULONG_PTR *>(e.slot), nullptr, e.slot_rva, {}};
        if (e.kind == PE_IMPORT_BY_ORDINAL) {
            s.fn = reinterpret_cast<void *>(GetProcAddress(lib, MAKEINTRESOURCEA(e.ordinal)));
            snprintf(s.how, sizeof s.how, "ordinal %u", unsigned(e.ordinal));
        } else if (e.kind == PE_IMPORT_BY_NAME) {
            s.fn = reinterpret_cast<void *>(GetProcAddress(lib, e.name));
            snprintf(s.how, sizeof s.how, "name");
        } else {
            s.fn = reinterpret_cast<void *>(*s.slot);
            snprintf(s.how, sizeof s.how, "bound");
        }
        visit(s);
    }
    if (skipped)
        log_import_skip(each_import_skip_logged, "[imports]", "game exe", first, skipped);
    return it.table;
}

}
