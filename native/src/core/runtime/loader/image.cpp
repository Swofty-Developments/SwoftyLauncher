#include "core/runtime/loader/image.hpp"

#include <algorithm>
#include <cstring>

#include "Zydis.h"
#include "core/runtime/scan_cache.hpp"

namespace runtime::loader {

GameImage &image()
{
    static GameImage instance;
    return instance;
}

const IMAGE_NT_HEADERS *GameImage::nt_headers() const
{
    return reinterpret_cast<const IMAGE_NT_HEADERS *>(base_ + reinterpret_cast<const IMAGE_DOS_HEADER *>(base_)->e_lfanew);
}

const IMAGE_SECTION_HEADER *GameImage::section_of(uintptr_t a) const
{
    auto nt = nt_headers();
    auto s = IMAGE_FIRST_SECTION(nt);
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++)
        if (a - (base_ + s->VirtualAddress) < (s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData))
            return s;
    return nullptr;
}

bool GameImage::section_named(uintptr_t a, const std::string &name) const
{
    auto s = section_of(a);
    return s && name.size() <= 8 && !strncmp(reinterpret_cast<const char *>(s->Name), name.c_str(), 8);
}

bool GameImage::is_code(uintptr_t a) const
{
    auto s = section_of(a);
    return s && (s->Characteristics & IMAGE_SCN_MEM_EXECUTE) && !(s->Characteristics & IMAGE_SCN_MEM_WRITE);
}

bool GameImage::in_exec(uintptr_t a) const
{
    auto s = section_of(a);
    return s && (s->Characteristics & IMAGE_SCN_MEM_EXECUTE);
}

uintptr_t GameImage::section_end(const IMAGE_SECTION_HEADER *s) const
{
    return base_ + s->VirtualAddress + (s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData);
}

uintptr_t GameImage::locate_string(const std::string &str) const
{
    std::string want = str;
    want.push_back('\0');
    auto nt = nt_headers();
    auto s = IMAGE_FIRST_SECTION(nt);
    uintptr_t hit = 0;
    int hits = 0;
    for (int i = 0; i < nt->FileHeader.NumberOfSections && hits < 2; i++, s++) {
        DWORD ch = s->Characteristics;
        if (!(ch & IMAGE_SCN_MEM_READ) || (ch & IMAGE_SCN_MEM_EXECUTE))
            continue;
        each_committed(base_ + s->VirtualAddress, section_end(s), [&](uintptr_t p, size_t n) {
            if (n < want.size())
                return;
            const uint8_t *lo = reinterpret_cast<const uint8_t *>(p), *last = lo + n - want.size();
            for (const uint8_t *q = lo; q <= last;) {
                const uint8_t *f = reinterpret_cast<const uint8_t *>(memchr(q, want[0], last - q + 1));
                if (!f)
                    break;
                if (!memcmp(f, want.data(), want.size()) && hits++ == 0)
                    hit = reinterpret_cast<uintptr_t>(f);
                q = f + 1;
            }
        });
    }
    return hits == 1 ? hit : 0;
}

uintptr_t GameImage::function_base_of(uintptr_t site) const
{
    struct RtFunc {
        uint32_t begin, end, unwind;
    };
    auto nt = nt_headers();
    auto &dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXCEPTION];
    if (!dir.VirtualAddress || dir.Size < sizeof(RtFunc))
        return 0;
    auto *fn = reinterpret_cast<const RtFunc *>(base_ + dir.VirtualAddress);
    size_t n = dir.Size / sizeof(RtFunc), lo = 0, hi = n;
    uint32_t r = static_cast<uint32_t>(site - base_);
    while (lo < hi) {
        size_t mid = (lo + hi) / 2;
        if (fn[mid].begin <= r)
            lo = mid + 1;
        else
            hi = mid;
    }
    if (lo && fn[lo - 1].begin <= r && r < fn[lo - 1].end)
        return base_ + fn[lo - 1].begin;
    return 0;
}

const IMAGE_SECTION_HEADER *GameImage::section_by_name(const std::string &name) const
{
    auto nt = nt_headers();
    auto s = IMAGE_FIRST_SECTION(nt);
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++)
        if (name.size() <= 8 && !strncmp(reinterpret_cast<const char *>(s->Name), name.c_str(), 8))
            return s;
    return nullptr;
}

const IMAGE_SECTION_HEADER *GameImage::code_section() const
{
    auto nt = nt_headers();
    auto s = IMAGE_FIRST_SECTION(nt);
    const IMAGE_SECTION_HEADER *exec = nullptr;
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++) {
        DWORD c = s->Characteristics;
        if ((c & IMAGE_SCN_MEM_EXECUTE) && (c & IMAGE_SCN_MEM_READ) && !(c & IMAGE_SCN_MEM_WRITE))
            return s;
        if ((c & IMAGE_SCN_MEM_EXECUTE) && !exec)
            exec = s;
    }
    return exec;
}

/* The key names the exe by fields that stay put while it runs: a packer may rewrite section flags, sizes or
   the image base in memory, and the decrypted-stage scans must still be found by the next launch. */
uint64_t GameImage::exe_key(long long exe_bytes) const
{
    auto nt = nt_headers();
    auto &fh = nt->FileHeader;
    auto &oh = nt->OptionalHeader;
    uint64_t k = runtime::fnv64(&fh.Machine, sizeof fh.Machine);
    k = runtime::fnv64(&fh.NumberOfSections, sizeof fh.NumberOfSections, k);
    k = runtime::fnv64(&fh.TimeDateStamp, sizeof fh.TimeDateStamp, k);
    k = runtime::fnv64(&oh.AddressOfEntryPoint, sizeof oh.AddressOfEntryPoint, k);
    k = runtime::fnv64(&oh.SizeOfImage, sizeof oh.SizeOfImage, k);
    k = runtime::fnv64(&oh.CheckSum, sizeof oh.CheckSum, k);
    k = runtime::fnv64(&exe_bytes, sizeof exe_bytes, k);
    auto s = IMAGE_FIRST_SECTION(nt);
    for (int i = 0; i < fh.NumberOfSections; i++, s++) {
        k = runtime::fnv64(s->Name, sizeof s->Name, k);
        k = runtime::fnv64(&s->VirtualAddress, sizeof s->VirtualAddress, k);
        k = runtime::fnv64(&s->SizeOfRawData, sizeof s->SizeOfRawData, k);
        k = runtime::fnv64(&s->PointerToRawData, sizeof s->PointerToRawData, k);
    }
    return k;
}

std::string GameImage::section_name(const IMAGE_SECTION_HEADER *s)
{
    char n[9] = {};
    if (s)
        memcpy(n, s->Name, 8);
    return n;
}

namespace {

thread_local ProtectionSpans *pass_spans;

}

ImageProtectionPass::ImageProtectionPass() : outer_(pass_spans)
{
    if (!outer_)
        pass_spans = &spans_;
}

ImageProtectionPass::~ImageProtectionPass()
{
    if (!outer_)
        pass_spans = nullptr;
}

bool page_readable(uintptr_t p, size_t n)
{
    if (pass_spans) {
        auto known = pass_spans->lookup(p, n);
        if (known != ProtectionSpans::Answer::unknown)
            return known == ProtectionSpans::Answer::readable;
    }
    MEMORY_BASIC_INFORMATION m{};
    if (!VirtualQuery(reinterpret_cast<void *>(p), &m, sizeof m))
        return false;
    bool readable = pages::readable_region(m);
    uintptr_t end = reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
    if (pass_spans && m.Type == MEM_IMAGE && reinterpret_cast<uintptr_t>(m.AllocationBase) == image().base())
        pass_spans->remember(reinterpret_cast<uintptr_t>(m.BaseAddress), end, readable);
    return readable && p + n <= end;
}

size_t insn_span(uintptr_t a, size_t min)
{
    ZydisDecoder d;
    ZydisDecoderInit(&d, ZYDIS_MACHINE_MODE_LONG_64, ZYDIS_STACK_WIDTH_64);
    size_t n = 0;
    while (n < min) {
        ZydisDecodedInstruction i;
        if (!ZYAN_SUCCESS(ZydisDecoderDecodeInstruction(&d, nullptr, reinterpret_cast<void *>(a + n), 15, &i)))
            return std::max(n, min);
        n += i.length;
    }
    return n;
}

}
