#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

namespace runtime::era {

// The mandatory tyre change per box (content.package rules.mandatory_tyre_change): GameSetupMode +0x8b, +0x98 and
// +0xa5 hold the race, race_2 and race_3 requirement flags, 13 each, indexed by the race length; a race session's
// TyreRegulations rule is on when its length's flag is set (read at race session start). The era picker writes all
// 39 flags of every swofty_f1 mode and puts the game's back (only where the bytes are still the ones written).
constexpr size_t MODE_TYRE_REQUIREMENTS = 0x8b, TYRE_REQUIREMENT_BYTES = 39;

class TyreChangeSwaps {
public:
    void swap(uint8_t *mode, bool required)
    {
        if (!mode)
            return;
        uint8_t *field = mode + MODE_TYRE_REQUIREMENTS;
        Swap *s = find(mode);
        if (!s) {
            swaps_.push_back({mode, {}, {}});
            s = &swaps_.back();
            std::memcpy(s->original, field, TYRE_REQUIREMENT_BYTES);
        }
        std::memset(s->written, required ? 1 : 0, TYRE_REQUIREMENT_BYTES);
        std::memcpy(field, s->written, TYRE_REQUIREMENT_BYTES);
    }

    size_t put_back()
    {
        size_t n = 0;
        for (auto &s : swaps_) {
            uint8_t *field = s.mode + MODE_TYRE_REQUIREMENTS;
            if (std::memcmp(field, s.written, TYRE_REQUIREMENT_BYTES))
                continue;
            std::memcpy(field, s.original, TYRE_REQUIREMENT_BYTES);
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
        uint8_t original[TYRE_REQUIREMENT_BYTES], written[TYRE_REQUIREMENT_BYTES];
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
