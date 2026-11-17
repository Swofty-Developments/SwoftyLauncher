#pragma once

#include "core/guards/skips.hpp"

#include <cstddef>
#include <cstdint>

namespace runtime::pit_box {

constexpr uint32_t BOX_SLOTS = 40;
constexpr uint32_t REGISTRY_SLOTS = 26;
constexpr int LOG_FIRST = 8;
constexpr size_t KEYS = 32;

inline bool box_usable(uint32_t box)
{
    return box < BOX_SLOTS;
}

inline bool registry_full(uint32_t count)
{
    return count >= REGISTRY_SLOTS;
}

using Log = swofty_core::guards::Say;

struct Seen {
    uint32_t keys[KEYS] = {};
    size_t count = 0;
    int logged = 0;

    Log note(uint32_t key)
    {
        for (size_t i = 0; i < count; i++)
            if (keys[i] == key)
                return Log::none;
        if (count < KEYS)
            keys[count++] = key;
        if (logged >= LOG_FIRST)
            return Log::none;
        return swofty_core::guards::say(++logged, LOG_FIRST);
    }
};

}
