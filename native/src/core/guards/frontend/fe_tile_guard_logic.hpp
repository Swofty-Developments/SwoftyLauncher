#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

namespace runtime::fe_tile {

constexpr uint64_t SLOT_SIZE = 0x10;
constexpr uint64_t SLOT_RECORD = 8;
constexpr uint64_t RECORD_NAME = 0x28;
constexpr uint64_t NAME_CHARS = 0x10;
constexpr size_t MAX_SLOTS = 4096;
constexpr size_t LOG_LABELS = 32;

inline bool canonical(uint64_t p)
{
    uint64_t top = p >> 47;
    return top == 0 || top == 0x1ffff;
}

template <class Mem> bool list_present(Mem &mem, uint64_t first_slot, uint64_t end, const std::string &label)
{
    if (!first_slot || first_slot >= end)
        return false;
    size_t seen = 0;
    for (uint64_t slot = first_slot; slot < end && seen < MAX_SLOTS; slot += SLOT_SIZE, seen++) {
        uint64_t record = 0;
        if (!mem.u64(slot + SLOT_RECORD, record))
            return false;
        if (!record)
            continue;
        uint64_t name_box = 0, chars = 0;
        std::string name;
        if (!mem.u64(record + RECORD_NAME, name_box) || !mem.u64(name_box + NAME_CHARS, chars) || !mem.text(chars, name))
            return false;
        if (name == label)
            return true;
    }
    return false;
}

}
