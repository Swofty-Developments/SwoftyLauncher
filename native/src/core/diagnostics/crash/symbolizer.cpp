#include "core/diagnostics/crash/symbolizer.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/runtime/loader/introspection.hpp"

namespace crash {

constinit ModuleMap modules{};
constinit SymbolTable symbol_table{};

const char *code_name(DWORD c)
{
    switch (c) {
    case EXCEPTION_ACCESS_VIOLATION:
        return "ACCESS_VIOLATION";
    case EXCEPTION_IN_PAGE_ERROR:
        return "IN_PAGE_ERROR";
    case EXCEPTION_ILLEGAL_INSTRUCTION:
        return "ILLEGAL_INSTRUCTION";
    case EXCEPTION_PRIV_INSTRUCTION:
        return "PRIV_INSTRUCTION";
    case EXCEPTION_INT_DIVIDE_BY_ZERO:
        return "INT_DIVIDE_BY_ZERO";
    case EXCEPTION_INT_OVERFLOW:
        return "INT_OVERFLOW";
    case EXCEPTION_STACK_OVERFLOW:
        return "STACK_OVERFLOW";
    case EXCEPTION_ARRAY_BOUNDS_EXCEEDED:
        return "ARRAY_BOUNDS_EXCEEDED";
    case EXCEPTION_NONCONTINUABLE_EXCEPTION:
        return "NONCONTINUABLE_EXCEPTION";
    case EXCEPTION_DATATYPE_MISALIGNMENT:
        return "DATATYPE_MISALIGNMENT";
    case 0xC0000374:
        return "HEAP_CORRUPTION";
    case 0xC0000409:
        return "STACK_BUFFER_OVERRUN";
    case EXCEPTION_BREAKPOINT:
        return "BREAKPOINT";
    case 0xE06D7363:
        return "C++ exception";
    case CODE_EXIT:
        return "swofty exit report";
    case CODE_GAME_FATAL:
        return "swofty game fatal error";
    }
    return "exception";
}

bool fatal(DWORD c)
{
    return c == EXCEPTION_ACCESS_VIOLATION || c == EXCEPTION_IN_PAGE_ERROR || c == EXCEPTION_ILLEGAL_INSTRUCTION ||
           c == EXCEPTION_PRIV_INSTRUCTION || c == EXCEPTION_INT_DIVIDE_BY_ZERO || c == EXCEPTION_INT_OVERFLOW ||
           c == EXCEPTION_STACK_OVERFLOW || c == EXCEPTION_ARRAY_BOUNDS_EXCEEDED || c == EXCEPTION_NONCONTINUABLE_EXCEPTION ||
           c == EXCEPTION_DATATYPE_MISALIGNMENT || c == 0xC0000374 || c == 0xC0000409;
}

std::string module_name(HMODULE m)
{
    char path[MAX_PATH] = "?";
    GetModuleFileNameA(m, path, MAX_PATH);
    const char *slash = strrchr(path, '\\');
    return slash ? slash + 1 : path;
}

const IMAGE_SECTION_HEADER *section_of(uintptr_t mb, uintptr_t a)
{
    auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(mb);
    if (!readable(mb, sizeof *dos) || dos->e_magic != IMAGE_DOS_SIGNATURE ||
        !readable(mb + dos->e_lfanew, sizeof(IMAGE_NT_HEADERS)))
        return nullptr;
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(mb + dos->e_lfanew);
    auto s = IMAGE_FIRST_SECTION(nt);
    if (!readable(reinterpret_cast<uintptr_t>(s), nt->FileHeader.NumberOfSections * sizeof *s))
        return nullptr;
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++)
        if (a - (mb + s->VirtualAddress) < (s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData))
            return s;
    return nullptr;
}

void section_into(uintptr_t mb, uintptr_t a, char *name)
{
    auto s = section_of(mb, a);
    if (!s) {
        strcpy(name, "?");
        return;
    }
    memcpy(name, s->Name, 8);
    name[8] = 0;
}

std::string section_name(uintptr_t a)
{
    char s[9];
    section_into(reinterpret_cast<uintptr_t>(process.exe), a, s);
    return s;
}

PRUNTIME_FUNCTION primary(PRUNTIME_FUNCTION f, DWORD64 base)
{
    for (int i = 0; i < 8; i++) {
        auto ui = reinterpret_cast<const uint8_t *>(base + f->UnwindData);
        if (!readable(reinterpret_cast<uintptr_t>(ui), 4) || !((ui[0] >> 3) & UNW_FLAG_CHAININFO))
            break;
        f = reinterpret_cast<PRUNTIME_FUNCTION>(const_cast<uint8_t *>(ui) + 4 + 2 * ((ui[2] + 1) & ~1));
    }
    return f;
}

namespace {

const char *exact(const std::vector<Symbol> &table, uint32_t start)
{
    auto it = std::lower_bound(table.begin(), table.end(), start, [](const Symbol &x, uint32_t r) { return x.rva < r; });
    return it != table.end() && it->rva == start ? it->name.c_str() : nullptr;
}

}

const char *SymbolTable::name_at(uint32_t start) const
{
    if (const char *name = exact(symbols, start))
        return name;
    return bound_ready.load(std::memory_order_acquire) ? exact(bound, start) : nullptr;
}

void SymbolTable::adopt(std::vector<Symbol> named)
{
    if (matches || bound_ready.load(std::memory_order_acquire))
        return;
    std::sort(named.begin(), named.end(), [](const Symbol &a, const Symbol &b) { return a.rva < b.rva; });
    bound = std::move(named);
    bound_ready.store(true, std::memory_order_release);
    cl("[crash] %zu function name(s) from the binding set for this exe", bound.size());
}

std::string where(uintptr_t a)
{
    HMODULE m = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(a), &m) ||
        !m) {
        std::string owner = core_code_owner(a);
        return fmt("0x%llx", (unsigned long long)a) + (owner.empty() ? " (no module)" : " (" + owner + ")");
    }
    auto mb = reinterpret_cast<uintptr_t>(m);
    std::string s = fmt("%s+0x%llx", module_name(m).c_str(), (unsigned long long)(a - mb));
    if (m == process.exe)
        s += " " + section_name(a);
    DWORD64 ib = 0;
    auto f = RtlLookupFunctionEntry(a, &ib, nullptr);
    if (!f)
        return s;
    f = primary(f, ib);
    uint32_t start = f->BeginAddress, off = uint32_t(a - ib - start);
    if (m != process.exe) {
        std::string owner = core_code_owner(ib + start);
        return owner.empty() ? s : s + " (" + owner + fmt("+0x%x)", off);
    }
    if (const char *name = symbol_table.name_at(start))
        return s + fmt(" %s+0x%x", name, off);
    return s + fmt(" fn base+0x%x+0x%x", start, off);
}

void ModuleMap::snapshot()
{
    modules.nmods = 0;
    auto teb = reinterpret_cast<uintptr_t>(NtCurrentTeb());
    uintptr_t peb = *reinterpret_cast<uintptr_t *>(teb + 0x60), ldr = 0;
    if (!readable(peb + 0x18, 8) || !(ldr = *reinterpret_cast<uintptr_t *>(peb + 0x18)) || !readable(ldr + 0x10, 16))
        return;
    uintptr_t head = ldr + 0x10, e = *reinterpret_cast<uintptr_t *>(head);
    for (int i = 0; e != head && i < 1024 && modules.nmods < int(sizeof modules.mods / sizeof *modules.mods); i++) {
        if (!readable(e, 0x68))
            break;
        Mod &m = modules.mods[modules.nmods];
        m.base = *reinterpret_cast<uintptr_t *>(e + 0x30);
        m.size = *reinterpret_cast<uint32_t *>(e + 0x40);
        auto copy = [](uintptr_t us, char *dst, size_t cap) {
            uint16_t len = *reinterpret_cast<uint16_t *>(us);
            auto w = *reinterpret_cast<const wchar_t **>(us + 8);
            dst[0] = 0;
            if (!w || !readable(reinterpret_cast<uintptr_t>(w), len))
                return;
            int k = WideCharToMultiByte(CP_UTF8, 0, w, len / 2, dst, int(cap - 1), nullptr, nullptr);
            dst[k > 0 ? k : 0] = 0;
        };
        copy(e + 0x58, m.name, sizeof m.name);
        copy(e + 0x48, m.path, sizeof m.path);
        if (m.base && m.size)
            modules.nmods++;
        e = *reinterpret_cast<uintptr_t *>(e);
    }
    std::sort(modules.mods, modules.mods + modules.nmods, [](const Mod &a, const Mod &b) { return a.base < b.base; });
}

const Mod *ModuleMap::at(uintptr_t a) const
{
    const Mod *m =
        std::upper_bound(modules.mods, modules.mods + modules.nmods, a, [](uintptr_t x, const Mod &y) { return x < y.base; });
    if (m == modules.mods)
        return nullptr;
    --m;
    return a - m->base < m->size ? m : nullptr;
}

void where_into(char *outb, size_t cap, uintptr_t a, bool owners)
{
    const Mod *m = modules.at(a);
    if (!m) {
        std::string owner = owners ? core_code_owner(a) : std::string();
        snprintf(outb, cap, "0x%llx (%s)", (unsigned long long)a, owner.empty() ? "no module" : owner.c_str());
        return;
    }
    char sec[9];
    section_into(m->base, a, sec);
    int k = snprintf(outb, cap, "%s+0x%llx %s", m->name, (unsigned long long)(a - m->base), sec);
    if (k < 0 || size_t(k) >= cap)
        return;
    DWORD64 ib = 0;
    auto f = RtlLookupFunctionEntry(a, &ib, nullptr);
    if (!f)
        return;
    f = primary(f, ib);
    uint32_t start = f->BeginAddress, off = uint32_t(a - ib - start);
    if (m->base == process.exe_base) {
        const char *name = symbol_table.name_at(start);
        if (name)
            snprintf(outb + k, cap - k, " %s+0x%x", name, off);
        else
            snprintf(outb + k, cap - k, " fn base+0x%x+0x%x", start, off);
        return;
    }
    std::string owner = owners ? core_code_owner(ib + start) : std::string();
    if (!owner.empty())
        snprintf(outb + k, cap - k, " (%s+0x%x)", owner.c_str(), off);
    else
        snprintf(outb + k, cap - k, " fn +0x%x+0x%x", start, off);
}

bool code_at(uintptr_t a)
{
    const Mod *m = modules.at(a);
    if (!m)
        return false;
    auto s = section_of(m->base, a);
    return s && (s->Characteristics & IMAGE_SCN_MEM_EXECUTE);
}

void SymbolTable::load(const std::wstring &path, DWORD stamp)
{
    FILE *f = _wfopen(path.c_str(), L"rb");
    if (!f)
        return cl("[crash] symbols.tsv missing; exe frames show rva only");
    char line[512];
    bool match = false;
    while (fgets(line, sizeof line, f)) {
        char name[256];
        unsigned rva;
        unsigned long ts;
        if (line[0] == '#') {
            match = sscanf(line, "# %*s %*s %lu", &ts) == 1 && ts == stamp;
            matches |= match;
        } else if (match && sscanf(line, "%x\t%255[^\t\r\n]", &rva, name) == 2) {
            symbol_table.symbols.push_back({rva, name});
        }
    }
    fclose(f);
    std::sort(symbol_table.symbols.begin(), symbol_table.symbols.end(),
              [](const Symbol &a, const Symbol &b) { return a.rva < b.rva; });
    cl(match ? "[crash] symbols.tsv: %zu KB function name(s) for this exe"
             : "[crash] symbols.tsv is for another build (%zu names loaded); exe frames show rva only",
       symbol_table.symbols.size());
}

}
