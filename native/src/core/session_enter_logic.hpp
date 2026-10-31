#pragma once

#include <cstddef>
#include <cstdint>

namespace runtime::session_enter {

constexpr uint64_t FOUND_AT = 0x118;
constexpr uint64_t MANAGER_AT = 0x30;
constexpr uint64_t INDEX_AT = 0x60;
constexpr uint64_t USER_BASE = 0x10000;
constexpr uint64_t USER_END = 0x800000000000ull;
constexpr uint32_t NO_INDEX = 0xffffffffu;
constexpr int LOG_FIRST = 4;

using Read = bool (*)(uint64_t address, void *out, size_t n, void *user);

struct Skip {
    bool skip = false;
    uint64_t manager = 0;
    uint32_t index = NO_INDEX;
};

inline Skip decide(uint64_t rsp, Read read, void *user)
{
    Skip s;
    uint64_t found = 0;
    if (!read(rsp + FOUND_AT, &found, sizeof found, user) || found != 0)
        return s;
    uint64_t manager = 0;
    if (!read(rsp + MANAGER_AT, &manager, sizeof manager, user) || manager < USER_BASE || manager >= USER_END)
        return s;
    uint64_t index = 0;
    if (read(rsp + INDEX_AT, &index, sizeof index, user))
        s.index = static_cast<uint32_t>(index & 0xff);
    s.skip = true;
    s.manager = manager;
    return s;
}

struct Skips {
    int count = 0;

    int note()
    {
        return ++count;
    }

    bool logged(int n) const
    {
        return n <= LOG_FIRST;
    }

    bool last(int n) const
    {
        return n == LOG_FIRST;
    }
};

}
