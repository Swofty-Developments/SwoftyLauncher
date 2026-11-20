#pragma once

#include "core/hooks/guard.hpp"
#include "core/runtime/pages.hpp"

#include <cstddef>
#include <cstdint>
#include <cstring>

// The memory callbacks guards hand their logic headers (bool(uint64_t address, void *, size_t n, void *)), one per
// check. None of them makes a copy safe against a page that changes after its check.
namespace swofty_core::guards {

// Copies the n bytes at address into out when the core's page table says they are readable (runtime::readable).
inline bool read_if_readable(uint64_t address, void *out, size_t n, void *)
{
    if (!::runtime::readable(reinterpret_cast<const void *>(address), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

// Copies n bytes from in to address when the core's page table says they are readable. It does not ask whether
// they are writable, so a readable page that is not writable faults in the copy. Its one caller, ai_line_set's
// repair, writes into the game's AI build records on its heap, whose pages are read-write. A caller that may write
// anywhere else asks runtime::writable_in_region (runtime/pages.hpp) instead.
inline bool write_if_readable(uint64_t address, const void *in, size_t n, void *)
{
    if (!::runtime::readable(reinterpret_cast<const void *>(address), n))
        return false;
    std::memcpy(reinterpret_cast<void *>(address), in, n);
    return true;
}

// Copies the n bytes at a non-null address whose range does not wrap into out, catching an access violation
// (hooks::guard::copy_memory) instead of checking pages first.
inline bool read_catching_faults(uint64_t address, void *out, size_t n, void *)
{
    if (!address || address + n < address)
        return false;
    return ::swofty_core::hooks::guard::copy_memory(out, reinterpret_cast<const void *>(address), n);
}

}
