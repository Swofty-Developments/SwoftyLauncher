// Test for the import table walk in native/include/pe_imports.h, the one the core's VirtualQuery accounting
// (perf.cpp), its import hooks (imports.hpp) and the loader's trigger slots (version.c) use. Crafted images sit in
// front of no-access guard pages and carry entries another module has rewritten: name entries holding pointers into
// a DLL image, name RVAs past SizeOfImage or in a no-access page, unterminated strings at the image's end, tables
// and slots outside the image. The walk must skip each one without touching it and still patch every valid slot.
// Built for the host and for Windows by native/test/run_pe_imports_test.sh; on Windows (and under Wine) readable()
// is the core's shared page table and the test also scribbles over its own exe's import name table.
#include "pe_imports.h"

#include <cstdint>
#include <cstdio>
#include <cstring>

#ifdef _WIN32
#include <swofty/mem.hpp>
#else
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>
#endif

static int failures;

static void expect(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    fflush(stdout);
    failures += !ok;
}

#ifdef _WIN32

static uint8_t *map_rw(size_t n)
{
    return static_cast<uint8_t *>(VirtualAlloc(nullptr, n, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
}

static void no_access(void *p, size_t n)
{
    DWORD old;
    VirtualProtect(p, n, PAGE_NOACCESS, &old);
    swofty::mem::shared::invalidate();
}

static int readable(uintptr_t at, size_t n, void *)
{
    return swofty::mem::shared::readable(at, n);
}

#else

static int probe[2];

static uint8_t *map_rw(size_t n)
{
    void *p = mmap(nullptr, n, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    return p == MAP_FAILED ? nullptr : static_cast<uint8_t *>(p);
}

static void no_access(void *p, size_t n)
{
    mprotect(p, n, PROT_NONE);
}

static int readable(uintptr_t at, size_t n, void *)
{
    char sink[4096];
    if (!n)
        n = 1;
    while (n) {
        size_t c = n > sizeof sink ? sizeof sink : n;
        ssize_t w = write(probe[1], reinterpret_cast<const void *>(at), c);
        if (w > 0 && read(probe[0], sink, size_t(w)) != w)
            return 0;
        if (w != ssize_t(c))
            return 0;
        at += c;
        n -= c;
    }
    return 1;
}

#endif

static int plain_write(uint64_t *slot, const void *value, void *)
{
    *slot = uint64_t(uintptr_t(value));
    return 1;
}

struct Img {
    uint8_t *p;
    uint32_t size;

    uintptr_t base() const { return uintptr_t(p); }
    void u16(uint32_t rva, uint16_t v) { memcpy(p + rva, &v, sizeof v); }
    void u32(uint32_t rva, uint32_t v) { memcpy(p + rva, &v, sizeof v); }
    void u64(uint32_t rva, uint64_t v) { memcpy(p + rva, &v, sizeof v); }
    void str(uint32_t rva, const char *s) { memcpy(p + rva, s, strlen(s) + 1); }
    void by_name(uint32_t rva, const char *s)
    {
        u16(rva, 0);
        str(rva + 2, s);
    }
    void desc(uint32_t at, uint32_t oft, uint32_t name, uint32_t ft)
    {
        u32(at, oft);
        u32(at + 12, name);
        u32(at + 16, ft);
    }
    uint64_t slot(uint32_t rva) const
    {
        uint64_t v;
        memcpy(&v, p + rva, sizeof v);
        return v;
    }
};

static Img image(uint32_t size, uint32_t guard, uint32_t dir)
{
    Img m{map_rw(size + guard), size};
    no_access(m.p + size, guard);
    m.u16(0, 0x5A4D);
    m.u32(0x3C, 0x80);
    m.u32(0x80, 0x4550);
    m.u16(0x84, 0x8664);
    m.u16(0x98, 0x20B);
    m.u32(0x98 + 0x38, size);
    m.u32(0x98 + 0x6C, 16);
    m.u32(0x98 + 0x78, dir);
    return m;
}

static const void *ptr(uint64_t v)
{
    return reinterpret_cast<const void *>(uintptr_t(v));
}

constexpr uint64_t VQ = 0x7ff812340010ull, VQ_BASE = 0x7ff812350020ull, OTHER = 0x7ff8aaaa0000ull,
                   REPL = 0x1111222233334444ull;

static void rewritten_entries()
{
    constexpr uint32_t SIZE = 0x6000, HOLE = 0x4000, TAIL = 0x5F00;
    Img m = image(SIZE, 0x2000, 0x1000);
    uint8_t *dll = map_rw(0x30000);
    const uint64_t in_dll_a = uint64_t(uintptr_t(dll)) + 0x2c120, in_dll_b = uint64_t(uintptr_t(dll)) + 0x2c150;

    m.str(0x3000, "KERNEL32.dll");
    m.str(0x3010, "KERNELBASE.dll");
    m.str(0x3020, "UNNAMED.dll");
    m.str(0x3030, "OTHER.dll");
    m.by_name(0x3100, "VirtualQuery");
    m.by_name(0x3120, "VirtualAlloc");
    memset(m.p + TAIL, 'V', SIZE - TAIL);

    m.desc(0x1000, 0x2000, 0x3000, 0x2800);
    m.u64(0x2000, 0x3100);
    m.u64(0x2008, in_dll_a);
    m.u64(0x2010, in_dll_b);
    m.u64(0x2018, SIZE + 0x10);
    m.u64(0x2020, 0x3100);
    m.u64(0x2028, 0x3120);
    m.u64(0x2030, 0x8000000000000005ull);
    for (uint32_t k = 0; k < 7; k++)
        m.u64(0x2800 + 8 * k, k == 4 ? VQ_BASE : VQ);

    m.desc(0x1014, 0x2100, 0x3010, 0x2900);
    m.u64(0x2100, HOLE);
    m.u64(0x2108, 0x3100);
    m.u64(0x2110, 0x3100);
    m.u64(0x2900, VQ);
    m.u64(0x2908, OTHER);
    m.u64(0x2910, VQ);

    m.desc(0x1028, 0x2200, 0x7ffa0000, 0x2A00);
    m.u64(0x2200, 0x3100);
    m.u64(0x2A00, VQ);

    m.desc(0x103C, 0, 0x3020, 0x2B00);
    m.u64(0x2B00, VQ);
    m.u64(0x2B08, OTHER);

    m.desc(0x1050, 0x2300, 0x3030, 0x2C00);
    m.u64(0x2300, TAIL);
    m.u64(0x2C00, VQ);

    m.desc(0x1064, SIZE - 4, 0x3030, 0x2D00);
    m.u64(0x2D00, VQ);

    m.desc(0x1078, 0x2400, 0x3030, 0x7ff00000);
    m.u64(0x2400, 0x3100);

    m.desc(0x108C, 0x2500, TAIL, 0x2E00);
    m.u64(0x2500, 0x3100);
    m.u64(0x2E00, VQ);

    no_access(m.p + HOLE, 0x1000);

    const void *match[] = {ptr(VQ), ptr(VQ_BASE)};
    pe_patch p{};
    p.fn = "VirtualQuery";
    p.match = match;
    p.matches = 2;
    p.repl = ptr(REPL);
    p.write = plain_write;
    int done = pe_patch_imports(m.base(), readable, nullptr, &p);

    expect(done == 4, "four valid slots are patched among the rewritten entries");
    expect(m.slot(0x2800) == REPL, "KERNEL32!VirtualQuery ahead of the rewritten entries is patched");
    expect(m.slot(0x2808) == VQ && m.slot(0x2810) == VQ,
           "the two slots whose name entries point into a DLL image are left alone");
    expect(m.slot(0x2818) == VQ, "the slot whose name RVA lies past SizeOfImage is left alone");
    expect(m.slot(0x2820) == REPL, "KERNEL32!VirtualQuery after the rewritten entries is patched");
    expect(m.slot(0x2828) == VQ, "a slot imported under another name is left alone");
    expect(m.slot(0x2830) == VQ, "a slot imported by ordinal is left alone");
    expect(m.slot(0x2900) == VQ, "the slot whose name lies in a no-access page is left alone");
    expect(m.slot(0x2908) == OTHER, "a slot another hook already holds is left alone");
    expect(m.slot(0x2910) == REPL, "KERNELBASE!VirtualQuery after the no-access name is patched");
    expect(m.slot(0x2A00) == VQ, "a descriptor whose DLL name lies past SizeOfImage is skipped whole");
    expect(m.slot(0x2B00) == REPL && m.slot(0x2B08) == OTHER, "a descriptor without a name table patches by value");
    expect(m.slot(0x2C00) == VQ, "a name that runs to the end of the image unterminated is skipped");
    expect(m.slot(0x2D00) == VQ, "a name table that runs past the image is skipped");
    expect(m.slot(0x2E00) == VQ, "a descriptor whose DLL name runs to the end of the image is skipped");
    expect(p.skipped == 9, "nine corrupt entries are counted");
    expect(p.first_skip.table == 1 && p.first_skip.index == 1 && p.first_skip.value == in_dll_a &&
               p.first_skip.value_rva == 0x2008 && p.first_skip.dll && !strcmp(p.first_skip.dll, "KERNEL32.dll") &&
               p.first_skip.why && !strcmp(p.first_skip.why, "import name outside the image"),
           "the first skip names the table, entry, value and where it was read");

    pe_imports it;
    pe_import e;
    int entries = 0, skips = 0, slotted_skips = 0, r;
    pe_imports_open(&it, m.base(), nullptr, readable, nullptr);
    while ((r = pe_imports_next(&it, &e)) != PE_IMPORT_END && entries + skips < 100) {
        if (r == PE_IMPORT_SKIP) {
            skips++;
            slotted_skips += e.slot != nullptr;
            continue;
        }
        entries++;
    }
    expect(r == PE_IMPORT_END && entries == 8 && skips == 9, "the walk ends after 8 entries and 9 skips");
    expect(!slotted_skips, "a skipped entry hands out no slot");
}

static void table_past_the_image()
{
    Img m = image(0x2000, 0x1000, 0x2000 - 30);
    m.str(0x1000, "KERNEL32.dll");
    m.desc(0x2000 - 30, 0, 0x1000, 0x1800);
    m.u64(0x1800, VQ);
    pe_imports it;
    pe_import e;
    expect(pe_imports_open(&it, m.base(), nullptr, readable, nullptr), "an image whose table nears its end opens");
    int r1 = pe_imports_next(&it, &e), r2 = pe_imports_next(&it, &e), r3 = pe_imports_next(&it, &e);
    expect(r1 == PE_IMPORT_ENTRY && r2 == PE_IMPORT_SKIP && r3 == PE_IMPORT_END,
           "a descriptor table without a terminator stops at the image's end");

    Img beyond = image(0x2000, 0x1000, 0x7ff00000);
    expect(pe_imports_open(&it, beyond.base(), nullptr, readable, nullptr), "an image whose import RVA is past its end opens");
    r1 = pe_imports_next(&it, &e);
    r2 = pe_imports_next(&it, &e);
    expect(r1 == PE_IMPORT_SKIP && r2 == PE_IMPORT_END, "an import directory past SizeOfImage is one skip and the end");
}

static void bad_headers()
{
    pe_imports it;
    Img m = image(0x2000, 0x1000, 0x1000);
    m.u16(0, 0x1234);
    expect(!pe_imports_open(&it, m.base(), nullptr, readable, nullptr), "no MZ, no walk");
    m = image(0x2000, 0x1000, 0x1000);
    m.u32(0x3C, 0x2000);
    expect(!pe_imports_open(&it, m.base(), nullptr, readable, nullptr), "NT headers in the guard page, no walk");
    m = image(0x2000, 0x1000, 0x1000);
    m.u16(0x98, 0x10B);
    expect(!pe_imports_open(&it, m.base(), nullptr, readable, nullptr), "a PE32 image, no walk");
    m = image(0x2000, 0x1000, 0x1000);
    m.u32(0x98 + 0x6C, 1);
    expect(!pe_imports_open(&it, m.base(), nullptr, readable, nullptr), "no import directory entry, no walk");
    m = image(0x2000, 0x1000, 0);
    expect(!pe_imports_open(&it, m.base(), nullptr, readable, nullptr), "an empty import directory, no walk");
    expect(!pe_imports_open(&it, 0, nullptr, readable, nullptr), "a null base, no walk");
}

static void one_dll()
{
    Img m = image(0x3000, 0x1000, 0x1000);
    m.str(0x2000, "WS2_32.dll");
    m.str(0x2010, "ws2_32.dll");
    m.str(0x2020, "KERNEL32.dll");
    m.by_name(0x2100, "send");
    m.desc(0x1000, 0x1800, 0x2000, 0x1C00);
    m.u64(0x1800, 0x2100);
    m.u64(0x1808, 0x8000000000000017ull);
    m.u64(0x1C00, OTHER);
    m.u64(0x1C08, OTHER);
    m.desc(0x1014, 0, 0x2020, 0x1D00);
    m.u64(0x1D00, VQ);
    m.desc(0x1028, 0x1900, 0x2010, 0x1E00);
    m.u64(0x1900, 0x8000000000000009ull);
    m.u64(0x1E00, OTHER);
    pe_imports it;
    pe_import e;
    pe_imports_open(&it, m.base(), "ws2_32.dll", readable, nullptr);
    int r1 = pe_imports_next(&it, &e);
    expect(r1 == PE_IMPORT_ENTRY && e.table == 1 && e.kind == PE_IMPORT_BY_NAME && !strcmp(e.name, "send") &&
               e.slot_rva == 0x1C00,
           "WS2_32.dll matches ws2_32.dll and its named entry comes first");
    int r2 = pe_imports_next(&it, &e);
    expect(r2 == PE_IMPORT_ENTRY && e.table == 1 && e.kind == PE_IMPORT_BY_ORDINAL && e.ordinal == 23,
           "then its ordinal entry");
    int r3 = pe_imports_next(&it, &e);
    expect(r3 == PE_IMPORT_ENTRY && e.table == 2 && e.kind == PE_IMPORT_BY_ORDINAL && e.ordinal == 9 &&
               e.slot_rva == 0x1E00,
           "the second ws2_32 descriptor is table 2 and KERNEL32 is passed over");
    expect(pe_imports_next(&it, &e) == PE_IMPORT_END && it.table == 2, "two tables name the DLL");
}

#ifdef _WIN32

static int vq_calls;
using VqFn = SIZE_T(WINAPI *)(LPCVOID, PMEMORY_BASIC_INFORMATION, SIZE_T);
static VqFn real_vq;

static SIZE_T WINAPI counting_vq(LPCVOID p, PMEMORY_BASIC_INFORMATION m, SIZE_T n)
{
    vq_calls++;
    return real_vq(p, m, n);
}

static int protect_write(uint64_t *slot, const void *value, void *)
{
    DWORD old;
    if (!VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old))
        return 0;
    *slot = uint64_t(uintptr_t(value));
    VirtualProtect(slot, sizeof *slot, old, &old);
    return 1;
}

static void own_exe()
{
    uintptr_t self = uintptr_t(GetModuleHandleW(nullptr));
    HMODULE k32 = GetModuleHandleW(L"kernel32.dll"), kb = GetModuleHandleW(L"kernelbase.dll");
    real_vq = reinterpret_cast<VqFn>(reinterpret_cast<void *>(GetProcAddress(k32, "VirtualQuery")));
    uintptr_t in_dll = uintptr_t(GetModuleHandleW(L"ntdll.dll"));
    pe_imports it;
    pe_import e;
    uint64_t *scribble[2] = {};
    int found = 0, vq_slots = 0;
    expect(pe_imports_open(&it, self, nullptr, readable, nullptr), "the test exe's import table opens");
    while (pe_imports_next(&it, &e) == PE_IMPORT_ENTRY) {
        if (e.kind != PE_IMPORT_BY_NAME)
            continue;
        if (!strcmp(e.name, "VirtualQuery")) {
            vq_slots++;
            continue;
        }
        if (found < 2)
            scribble[found++] = reinterpret_cast<uint64_t *>(it.base + uintptr_t(it.names) + 8 * e.index);
    }
    expect(vq_slots >= 1 && found == 2, "the exe imports VirtualQuery by name and two other names");
    if (found < 2)
        return;
    for (int i = 0; i < 2; i++)
        protect_write(scribble[i], reinterpret_cast<const void *>(in_dll + 0x2c120 + 0x30 * i), nullptr);

    const void *match[] = {reinterpret_cast<const void *>(real_vq),
                           kb ? reinterpret_cast<const void *>(GetProcAddress(kb, "VirtualQuery")) : nullptr};
    pe_patch p{};
    p.fn = "VirtualQuery";
    p.match = match;
    p.matches = 2;
    p.repl = reinterpret_cast<const void *>(&counting_vq);
    p.write = protect_write;
    int done = pe_patch_imports(self, readable, nullptr, &p);
    expect(done >= 1, "the exe's VirtualQuery slot is patched with two name entries pointing into ntdll");
    expect(p.skipped == 2 && p.first_skip.value == in_dll + 0x2c120,
           "both name entries pointing into ntdll are skipped, the first one reported");
    MEMORY_BASIC_INFORMATION mbi{};
    vq_calls = 0;
    VirtualQuery(&mbi, &mbi, sizeof mbi);
    expect(vq_calls == 1, "a VirtualQuery call through the import slot reaches the replacement");
}

#endif

int main()
{
#ifndef _WIN32
    if (pipe(probe) || fcntl(probe[1], F_SETFL, O_NONBLOCK))
        return 2;
#endif
    rewritten_entries();
    table_past_the_image();
    bad_headers();
    one_dll();
#ifdef _WIN32
    own_exe();
#endif
    printf("%s: %d failure(s)\n", failures ? "FAIL" : "ok", failures);
    return failures ? 1 : 0;
}
