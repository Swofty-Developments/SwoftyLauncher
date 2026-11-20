#pragma once

#include <cstddef>
#include <cstdint>
#include <vector>

namespace runtime::era {

// A box's race time limit (content.package rules.race_time_limit_s) applies through the host setup mode's
// race_time_limit_in_seconds (+0xb4, three u32, re/db structs/GameSetupMode.h, copied by the mode loader 0x142a14f20
// from the comma list; classic 7200,7200,7200). The swap writes the box's seconds into all three and remembers the
// game's; put_back restores them wherever the three still hold the swap's value.
constexpr size_t MODE_RACE_TIME_LIMIT = 0xb4;
constexpr size_t RACE_TIME_LIMITS = 3;

class TimeLimitSwaps {
public:
    void swap(uint8_t *mode, uint32_t seconds)
    {
        if (!mode)
            return;
        uint32_t *field = limits(mode);
        Swap *s = find(mode);
        if (!s) {
            swaps_.push_back({mode, {field[0], field[1], field[2]}, seconds});
            s = &swaps_.back();
        }
        s->written = seconds;
        for (size_t i = 0; i < RACE_TIME_LIMITS; ++i)
            field[i] = seconds;
    }

    size_t put_back()
    {
        size_t n = 0;
        for (auto &s : swaps_) {
            uint32_t *field = limits(s.mode);
            bool ours = true;
            for (size_t i = 0; i < RACE_TIME_LIMITS; ++i)
                ours = ours && field[i] == s.written;
            if (!ours)
                continue;
            for (size_t i = 0; i < RACE_TIME_LIMITS; ++i)
                field[i] = s.original[i];
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
        uint32_t original[RACE_TIME_LIMITS];
        uint32_t written;
    };
    static uint32_t *limits(uint8_t *mode) { return reinterpret_cast<uint32_t *>(mode + MODE_RACE_TIME_LIMIT); }
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
