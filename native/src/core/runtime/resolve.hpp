#pragma once

#include <cstdint>
#include <string>

#include <swofty/mixin.hpp>

namespace runtime {

// The address this build resolves for `id`, or 0 with `id` added to the comma-separated `missing`.
inline uintptr_t resolve_or_note(const char *id, std::string &missing)
{
    uintptr_t a = swofty::api->resolve(id);
    if (!a)
        missing += std::string(missing.empty() ? "" : ", ") + id;
    return a;
}

}
