#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "core/runtime/pages.hpp"

namespace crash {

struct Symbol {
    uint32_t rva;
    std::string name;
};

// The function names for the frames in the exe: the KB names from symbols.tsv when it has a section for this exe's
// PE timestamp, otherwise the names of the binding set's resolved functions, which follow every build the set
// covers. The binding names are adopted once after the bindings resolve and published with release order, so a
// report written meanwhile sees either none or all of them.
class SymbolTable {
public:
    void load(const std::wstring &path, DWORD stamp);
    void adopt(std::vector<Symbol> named);
    const char *name_at(uint32_t start) const;
    size_t bound_count() const { return bound_ready.load(std::memory_order_acquire) ? bound.size() : 0; }

    std::vector<Symbol> symbols;
    bool matches = false;

private:
    std::vector<Symbol> bound;
    std::atomic<bool> bound_ready{false};
};

struct Mod {
    uintptr_t base;
    size_t size;
    char name[64];
    char path[260];
};

// The loaded modules, read from the PEB without the loader lock and sorted by base, so that a fault or exit path can
// name an address without calling into the loader.
class ModuleMap {
public:
    static constexpr int CAPACITY = 512;

    void snapshot();
    const Mod *at(uintptr_t a) const;

    Mod mods[CAPACITY];
    int nmods = 0;
};

extern ModuleMap modules;
extern SymbolTable symbol_table;

inline bool readable(uintptr_t a, size_t n) { return runtime::pages::readable_now(a, n); }

// Copies from memory that may be unmapped, asking Windows about each page once.
struct Reader {
    uintptr_t page = 1;
    bool ok = false;

    bool read(uintptr_t a, void *dst, size_t n)
    {
        if (!n || a + n < a)
            return false;
        for (uintptr_t p = a & ~0xfffull; p < a + n; p += 0x1000) {
            if (p != page) {
                page = p;
                ok = readable(p, 1);
            }
            if (!ok)
                return false;
        }
        memcpy(dst, reinterpret_cast<const void *>(a), n);
        return true;
    }
};

const char *code_name(DWORD c);
bool fatal(DWORD c);
std::string module_name(HMODULE m);
const IMAGE_SECTION_HEADER *section_of(uintptr_t mb, uintptr_t a);
void section_into(uintptr_t mb, uintptr_t a, char *name);
std::string section_name(uintptr_t a);
PRUNTIME_FUNCTION primary(PRUNTIME_FUNCTION f, DWORD64 base);
// An address as module+offset, section and function: where() asks the loader, where_into() reads only the module map.
std::string where(uintptr_t a);
void where_into(char *outb, size_t cap, uintptr_t a, bool owners);
bool code_at(uintptr_t a);

template <class F> int walk(CONTEXT ctx, uintptr_t lo, uintptr_t hi, int max, F frame)
{
    int n = 0;
    for (int i = 0; i < max && ctx.Rip; i++) {
        if (!frame(n++, ctx))
            break;
        if (ctx.Rsp < lo || ctx.Rsp >= hi)
            break;
        DWORD64 ib = 0, before = ctx.Rsp;
        auto f = RtlLookupFunctionEntry(ctx.Rip, &ib, nullptr);
        if (f) {
            void *hd;
            DWORD64 ef;
            RtlVirtualUnwind(UNW_FLAG_NHANDLER, ib, ctx.Rip, f, &ctx, &hd, &ef, nullptr);
        } else {
            if (!readable(ctx.Rsp, 8))
                break;
            ctx.Rip = *reinterpret_cast<DWORD64 *>(ctx.Rsp);
            ctx.Rsp += 8;
        }
        if (ctx.Rsp < before)
            break;
    }
    return n;
}

}
