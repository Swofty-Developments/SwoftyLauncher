// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

#include "weekend.hpp"

namespace f1990 {

// An entrants.characters row as the game keeps it in the character vector (vector object: begin
// +0x08, end +0x10): 0x38 bytes, stat_id byte at +0x00, string objects for name, full_name,
// first_name, last_name and job_role at +0x08..+0x28 (text at string +0x10).
constexpr size_t kCharacterSize = 0x38;
constexpr size_t kCharacterMax = 4096;

inline const char *character_name(const void *character)
{
    auto *s = at<const uint8_t *>(character, 0x08);
    return s ? at<const char *>(s, 0x10) : nullptr;
}

inline const uint8_t *find_character(const void *vector, const char *name)
{
    if (!vector || !name)
        return nullptr;
    auto *b = at<const uint8_t *>(vector, 0x08), *e = at<const uint8_t *>(vector, 0x10);
    if (!b || e < b || size_t(e - b) % kCharacterSize || size_t(e - b) / kCharacterSize > kCharacterMax)
        return nullptr;
    for (; b < e; b += kCharacterSize) {
        const char *n = character_name(b);
        if (n && !std::strcmp(n, name))
            return b;
    }
    return nullptr;
}

// The record the ref builder gets for the stock agent in a 1990 career: the 1990 agent's names, job
// and texture name, under the stock agent's stat_id so lookups by id still find the ref.
inline void stand_in_character(uint8_t (&out)[kCharacterSize], const uint8_t *stock, const uint8_t *replacement)
{
    std::memcpy(out, replacement, kCharacterSize);
    out[0] = stock[0];
}

}
