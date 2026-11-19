#pragma once

#include <cstddef>
#include <cstdint>

namespace runtime::cutscene_team {

constexpr uint64_t USER_BASE = 0x10000;
constexpr uint64_t USER_END = 0x800000000000ull;
constexpr size_t LOOKBACK = 0x60;
constexpr size_t LABEL_SLOTS = 16;
constexpr uint32_t SCALE_SHIFT = 4;

inline bool record_missing(uint64_t record) { return record < USER_BASE || record >= USER_END; }

constexpr uint32_t team_of(uint64_t scaled_id) { return uint32_t(scaled_id >> SCALE_SHIFT) & 0xffff; }

inline uint64_t no_team_target(const uint8_t *code, size_t n, uint64_t code_at, uint64_t site)
{
    for (size_t i = n >= 6 ? n - 6 : 0; i + 6 <= n; i--) {
        if (code[i] == 0x81 && code[i + 1] == 0xf9 && code[i + 2] == 0 && code[i + 3] == 0 && code[i + 4] == 0xff &&
            code[i + 5] == 0x3f) {
            size_t j = i + 6;
            uint64_t target = 0;
            if (j + 2 <= n && code[j] == 0x74) {
                target = code_at + j + 2 + uint64_t(int64_t(int8_t(code[j + 1])));
            } else if (j + 6 <= n && code[j] == 0x0f && code[j + 1] == 0x84) {
                int32_t rel = int32_t(uint32_t(code[j + 2]) | uint32_t(code[j + 3]) << 8 | uint32_t(code[j + 4]) << 16 |
                                      uint32_t(code[j + 5]) << 24);
                target = code_at + j + 6 + uint64_t(int64_t(rel));
            }
            return target > site ? target : 0;
        }
        if (i == 0)
            break;
    }
    return 0;
}

inline uint64_t resume_for(uint64_t record, uint64_t target) { return record_missing(record) ? target : 0; }

struct Seen {
    uint32_t keys[LABEL_SLOTS] = {};
    size_t count = 0;

    bool first(uint32_t site, uint32_t team)
    {
        uint32_t key = site << 16 | (team & 0xffff);
        for (size_t i = 0; i < count; i++)
            if (keys[i] == key)
                return false;
        if (count == LABEL_SLOTS)
            return false;
        keys[count++] = key;
        return true;
    }
};

}
