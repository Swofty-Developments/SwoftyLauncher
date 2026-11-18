#pragma once
#include <cstddef>
#include <cstdint>

// The write rule of runtime/pages.hpp without Windows, so the host tests run it. The protection values are the Windows
// PAGE_* constants.
namespace runtime::pages {

// PAGE_READWRITE, PAGE_WRITECOPY, PAGE_EXECUTE_READWRITE and PAGE_EXECUTE_WRITECOPY.
constexpr uint32_t WRITABLE_PROTECTION = 0x04 | 0x08 | 0x40 | 0x80;
constexpr uint32_t NOACCESS_PROTECTION = 0x01, GUARD_PROTECTION = 0x100;

// One VirtualQuery answer (committed or not, protection, region base and size) says whether n bytes at p can be written.
constexpr bool writable_answer(bool committed, uint32_t protect, uintptr_t base, uint64_t size, uintptr_t p, size_t n)
{
    if (!p || !committed || (protect & (NOACCESS_PROTECTION | GUARD_PROTECTION)) || !(protect & WRITABLE_PROTECTION))
        return false;
    return p >= base && p + n <= base + size;
}

}
