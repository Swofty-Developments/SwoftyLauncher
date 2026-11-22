#pragma once

#include <cstddef>
#include <cstdint>
#include <vector>

namespace runtime::era {

// A box's tyre warmer rule (content.package rules.tyre_warmers) applies through the host setup mode's
// has_tyre_warmers byte (+0x1c8, re/db structs/GameSetupMode.h, written by the mode loader 0x142a14f20). The swap
// writes the box's value and remembers the game's; put_back restores it wherever the byte still holds the swap's value.
constexpr size_t MODE_TYRE_WARMERS = 0x1c8;

class WarmerSwaps {
public:
    void swap(uint8_t *mode, bool warmers)
    {
        if (!mode)
            return;
        uint8_t &field = mode[MODE_TYRE_WARMERS];
        uint8_t want = warmers ? 1 : 0;
        Swap *s = find(mode);
        if (!s) {
            swaps_.push_back({mode, field, want});
            s = &swaps_.back();
        }
        s->written = want;
        field = want;
    }

    size_t put_back()
    {
        size_t n = 0;
        for (auto &s : swaps_) {
            uint8_t &field = s.mode[MODE_TYRE_WARMERS];
            if (field != s.written)
                continue;
            field = s.original;
            n++;
        }
        swaps_.clear();
        return n;
    }

    // Drops the swaps without reading or writing the modes, which the game may have freed.
    size_t forget()
    {
        size_t n = swaps_.size();
        swaps_.clear();
        return n;
    }

    size_t active() const { return swaps_.size(); }

private:
    struct Swap {
        uint8_t *mode;
        uint8_t original, written;
    };
    Swap *find(const uint8_t *mode)
    {
        for (auto &s : swaps_)
            if (s.mode == mode)
                return &s;
        return nullptr;
    }
    std::vector<Swap> swaps_;
};

}
