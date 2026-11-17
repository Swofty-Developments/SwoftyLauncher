#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <string>

#include "core/runtime/pages.hpp"
#include "core/runtime/protection_spans_logic.hpp"

namespace runtime::loader {

// The running game exe as mapped in memory: its headers, sections and the functions its exception
// directory names. Every address the binding and mixin machinery reasons about is measured from base().
class GameImage {
public:
    void attach(uintptr_t base) { base_ = base; }
    uintptr_t base() const { return base_; }
    unsigned long long rva(uintptr_t a) const { return a - base_; }

    const IMAGE_NT_HEADERS *nt_headers() const;
    const IMAGE_SECTION_HEADER *section_of(uintptr_t a) const;
    const IMAGE_SECTION_HEADER *section_by_name(const std::string &name) const;
    const IMAGE_SECTION_HEADER *code_section() const;
    bool section_named(uintptr_t a, const std::string &name) const;
    bool is_code(uintptr_t a) const;
    bool in_exec(uintptr_t a) const;
    uintptr_t section_end(const IMAGE_SECTION_HEADER *s) const;
    uintptr_t locate_string(const std::string &str) const;
    uintptr_t function_base_of(uintptr_t site) const;
    uint64_t exe_key(long long exe_bytes) const;

    static std::string section_name(const IMAGE_SECTION_HEADER *s);

private:
    uintptr_t base_ = 0;
};

GameImage &image();

/* Keeps the game image's VirtualQuery answers on this thread for one binding pass that installs nothing. */
class ImageProtectionPass {
public:
    ImageProtectionPass();
    ~ImageProtectionPass();
    ImageProtectionPass(const ImageProtectionPass &) = delete;
    ImageProtectionPass &operator=(const ImageProtectionPass &) = delete;

private:
    ProtectionSpans spans_;
    ProtectionSpans *outer_;
};

bool page_readable(uintptr_t p, size_t n);
size_t insn_span(uintptr_t a, size_t min);

inline uint8_t byte_at(uintptr_t a) { return *reinterpret_cast<const uint8_t *>(a); }

template <class F> void each_committed(uintptr_t lo, uintptr_t hi, F fn)
{
    for (uintptr_t a = lo; a < hi;) {
        MEMORY_BASIC_INFORMATION m{};
        if (!VirtualQuery(reinterpret_cast<void *>(a), &m, sizeof m))
            break;
        uintptr_t next = reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
        if (next > hi)
            next = hi;
        if (next <= a)
            break;
        if (pages::readable_region(m))
            fn(a, next - a);
        a = next;
    }
}

}
