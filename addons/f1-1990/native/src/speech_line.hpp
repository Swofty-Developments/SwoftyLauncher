// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstring>

#include "weekend.hpp"

namespace f1990 {

// The speaker of a speech line, as Game_SpeechSystem_PostLine names it: line +0x80 indexes the
// speech system's speaker records (*(speech +0x728) +0x60, 0x88 each, record pointer, name at
// +0x18) below the count at speech +0x7cc.
inline const char *speech_speaker(const void *speech, const void *line)
{
    uint64_t index = at<uint64_t>(line, 0x80);
    if (index >= at<uint32_t>(speech, 0x7cc))
        return nullptr;
    auto *table = at<const uint8_t *>(speech, 0x728);
    auto *record = table ? at<const uint8_t *>(table, 0x60 + index * 0x88) : nullptr;
    return record ? at<const char *>(record, 0x18) : nullptr;
}

inline bool is_agent_speaker(const char *speaker)
{
    return speaker && (!std::strcmp(speaker, "agent") || !std::strcmp(speaker, "agent_cutscene"));
}

}
