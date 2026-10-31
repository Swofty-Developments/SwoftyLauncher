// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace f1990 {

struct BodyRegistry {
    static constexpr size_t kCount = 0x5d8, kScheduler = 0x350;
    static constexpr uint32_t kSlots = 26;

    static uint32_t count(const uint8_t *registry)
    {
        uint32_t n;
        std::memcpy(&n, registry + kCount, sizeof n);
        return n;
    }

    static bool full(uint32_t count) { return count >= kSlots; }

    static bool scheduled(const uint8_t *registry)
    {
        uint64_t p;
        std::memcpy(&p, registry + kScheduler, sizeof p);
        return p != 0;
    }
};

}
