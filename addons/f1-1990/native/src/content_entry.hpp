// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

namespace f1990 {

constexpr size_t kContentEntrySize = 0x60;
constexpr uint32_t kNoContent = 0x3fff0000;
constexpr uint16_t kCarGroup1990 = 2;

struct ContentPick {
    enum Kind { none, exact, dlc } kind;
    uint32_t id;
};

// The career content entries (customisation car groups, 0x60 bytes: +0x04 packed id with the
// group's stat id in the high word, +0x58 dlc_content). The 1990 tile takes the group with the
// wanted stat id; without one it falls back to the first dlc group, as the 2026 tile picks.
inline ContentPick pick_content(const uint8_t *begin, const uint8_t *end, uint16_t stat_id)
{
    ContentPick fallback{ContentPick::none, kNoContent};
    for (const uint8_t *e = begin; e && e + kContentEntrySize <= end; e += kContentEntrySize) {
        uint32_t id;
        std::memcpy(&id, e + 4, sizeof id);
        if ((id >> 16) == stat_id)
            return {ContentPick::exact, id};
        if (fallback.kind == ContentPick::none && e[0x58])
            fallback = {ContentPick::dlc, id};
    }
    return fallback;
}

}
