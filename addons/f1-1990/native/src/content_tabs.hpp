// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

#include "weekend.hpp"

namespace f1990 {

// A UI content group's entries as the tab builder reads them: begin +0x08, end +0x10, 0x30 bytes
// each, the tab name an NeStr at +0x08 (text at +0x09). The builder makes tab i from entry i.
constexpr size_t kContentTabSize = 0x30;
constexpr size_t kContentTabMax = 64;

inline const char *content_tab_name(const uint8_t *entry)
{
    auto *s = at<const uint8_t *>(entry, 0x08);
    return s ? reinterpret_cast<const char *>(s + 9) : nullptr;
}

// Bit i is set when entry i is one of the named tabs; -1 when the group doesn't look like a content
// group.
template <size_t N> int64_t content_tab_mask(const void *group, const char *const (&names)[N])
{
    auto *b = at<const uint8_t *>(group, 0x08), *e = at<const uint8_t *>(group, 0x10);
    if (!b || e < b || size_t(e - b) % kContentTabSize || size_t(e - b) / kContentTabSize >= kContentTabMax)
        return -1;
    int64_t mask = 0;
    for (size_t i = 0; i < size_t(e - b) / kContentTabSize; i++) {
        const char *name = content_tab_name(b + i * kContentTabSize);
        for (const char *n : names)
            if (name && !std::strcmp(name, n))
                mask |= int64_t(1) << i;
    }
    return mask;
}

struct TabState {
    uint8_t content_visible, enabled;
};

// The state a header tab gets: a hidden tab is off whatever the view model asks, the way the game
// turns off a tab it doesn't offer.
inline TabState header_tab_state(uint64_t hidden, uint64_t tab, TabState asked)
{
    return tab < kContentTabMax && (hidden >> tab & 1) ? TabState{0, 0} : asked;
}

}
