#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

namespace runtime::era {

// A box's engine (apply/content/rev_lights.hpp) applies through the host setup mode's equal_performance_engine: the
// entrants.engines stat id at +0xfa and its set flag at +0xfb (re/db structs/GameSetupMode.h, written by the mode loader
// 0x142a14f20 as one u16, stat id | 0x100). The wheel LED builder reads both for every car running equal performance.
// The swap writes the box's stat id into a mode whose flag is set and remembers the game's; a mode without the flag
// already shows each car its own engine and is left alone. put_back restores the id wherever the field still holds the
// swap's value.
constexpr size_t MODE_EQUAL_ENGINE = 0xfa;
constexpr size_t MODE_EQUAL_ENGINE_SET = 0xfb;

class EngineSwaps {
public:
    // False when the mode has no equal_performance_engine to swap.
    bool swap(uint8_t *mode, uint8_t stat_id)
    {
        if (!mode)
            return false;
        Swap *s = find(mode);
        if (!s && !mode[MODE_EQUAL_ENGINE_SET])
            return false;
        if (!s) {
            swaps_.push_back({mode, mode[MODE_EQUAL_ENGINE], stat_id});
            s = &swaps_.back();
        }
        s->written = stat_id;
        mode[MODE_EQUAL_ENGINE] = stat_id;
        return true;
    }

    size_t put_back()
    {
        size_t n = 0;
        for (auto &s : swaps_) {
            if (s.mode[MODE_EQUAL_ENGINE] != s.written || !s.mode[MODE_EQUAL_ENGINE_SET])
                continue;
            s.mode[MODE_EQUAL_ENGINE] = s.original;
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
