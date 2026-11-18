#include "core/diagnostics/crash/stall_frames.hpp"

#include <cstdio>
#include <cstring>

#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/stall_walk.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"

namespace crash {

namespace {

struct Pdata {
    const RUNTIME_FUNCTION *f;
    size_t n;
};

constinit Pdata stall_pdata[ModuleMap::CAPACITY]{};

const IMAGE_DATA_DIRECTORY *image_dir(uintptr_t base, int which)
{
    auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base);
    if (!readable(base, sizeof *dos) || !readable(base + dos->e_lfanew, sizeof(IMAGE_NT_HEADERS64)))
        return nullptr;
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS64 *>(base + dos->e_lfanew);
    if (nt->OptionalHeader.Magic != IMAGE_NT_OPTIONAL_HDR64_MAGIC || nt->OptionalHeader.NumberOfRvaAndSizes <= DWORD(which))
        return nullptr;
    const IMAGE_DATA_DIRECTORY *d = &nt->OptionalHeader.DataDirectory[which];
    return d->VirtualAddress && d->Size ? d : nullptr;
}

/* The nearest export at or before a, for the frames in system and addon modules, which have no symbol table. */
const char *export_near(const Mod *m, uintptr_t a, uint32_t &off)
{
    const IMAGE_DATA_DIRECTORY *d = image_dir(m->base, IMAGE_DIRECTORY_ENTRY_EXPORT);
    if (!d || d->Size < sizeof(IMAGE_EXPORT_DIRECTORY) || uint64_t(d->VirtualAddress) + d->Size > m->size ||
        !readable(m->base + d->VirtualAddress, d->Size))
        return nullptr;
    auto ed = reinterpret_cast<const IMAGE_EXPORT_DIRECTORY *>(m->base + d->VirtualAddress);
    if (!ed->NumberOfNames || ed->NumberOfNames > 0x10000 || ed->NumberOfFunctions > 0x10000 ||
        !readable(m->base + ed->AddressOfFunctions, ed->NumberOfFunctions * 4ull) ||
        !readable(m->base + ed->AddressOfNames, ed->NumberOfNames * 4ull) ||
        !readable(m->base + ed->AddressOfNameOrdinals, ed->NumberOfNames * 2ull))
        return nullptr;
    auto funcs = reinterpret_cast<const DWORD *>(m->base + ed->AddressOfFunctions);
    auto names = reinterpret_cast<const DWORD *>(m->base + ed->AddressOfNames);
    auto ords = reinterpret_cast<const WORD *>(m->base + ed->AddressOfNameOrdinals);
    uint32_t rva = uint32_t(a - m->base), best = 0;
    const char *name = nullptr;
    for (DWORD i = 0; i < ed->NumberOfNames; i++) {
        WORD o = ords[i];
        if (o >= ed->NumberOfFunctions)
            continue;
        DWORD f = funcs[o];
        if (f > rva || f < best || (f >= d->VirtualAddress && f < d->VirtualAddress + d->Size))
            continue;
        if (names[i] < d->VirtualAddress || names[i] >= d->VirtualAddress + d->Size)
            continue;
        best = f;
        name = reinterpret_cast<const char *>(m->base + names[i]);
    }
    if (!name || rva - best >= 0x20000)
        return nullptr;
    off = rva - best;
    return name;
}

}

PRUNTIME_FUNCTION stall_lookup(DWORD64 pc, DWORD64 *base)
{
    const Mod *m = modules.at(uintptr_t(pc));
    if (!m)
        return nullptr;
    const Pdata &p = stall_pdata[m - modules.mods];
    uint32_t rva = uint32_t(pc - m->base);
    size_t lo = 0, hi = p.f ? p.n : 0;
    while (lo < hi) {
        size_t mid = lo + (hi - lo) / 2;
        if (rva < p.f[mid].BeginAddress)
            hi = mid;
        else if (rva >= p.f[mid].EndAddress)
            lo = mid + 1;
        else {
            *base = m->base;
            return const_cast<PRUNTIME_FUNCTION>(&p.f[mid]);
        }
    }
    return nullptr;
}

void stall_tables()
{
    for (int i = 0; i < modules.nmods; i++) {
        const Mod &m = modules.mods[i];
        stall_pdata[i] = {nullptr, 0};
        const IMAGE_DATA_DIRECTORY *d = image_dir(m.base, IMAGE_DIRECTORY_ENTRY_EXCEPTION);
        if (!d || d->Size < sizeof(RUNTIME_FUNCTION) || uint64_t(d->VirtualAddress) + d->Size > m.size ||
            !readable(m.base + d->VirtualAddress, d->Size))
            continue;
        stall_pdata[i] = {reinterpret_cast<const RUNTIME_FUNCTION *>(m.base + d->VirtualAddress),
                          d->Size / sizeof(RUNTIME_FUNCTION)};
    }
}

const char *export_of(uint64_t ip, uint32_t &off)
{
    const Mod *m = modules.at(ip);
    if (!m || m->base == process.exe_base)
        return nullptr;
    const char *name = export_near(m, ip, off);
    if (!name)
        return nullptr;
    DWORD64 base = 0;
    PRUNTIME_FUNCTION f = stall_lookup(ip, &base);
    if (f)
        f = primary(f, base);
    uint32_t rva = uint32_t(ip - m->base);
    return f ? (f->BeginAddress == rva - off ? name : nullptr) : (off < 0x100 ? name : nullptr);
}

void stall_frame(char *w, size_t cap, uint64_t ip)
{
    where_into(w, cap, ip, true);
    uint32_t off = 0;
    if (const char *name = export_of(ip, off)) {
        size_t k = strlen(w);
        snprintf(w + k, cap - k, " [%s+0x%x]", name, off);
    }
}

}
