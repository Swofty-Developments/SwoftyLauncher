#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <utility>

namespace runtime::era {

// A box's weekend (content.package rules.weekend_mode) applies by exchanging the weekend options container of the host
// setup mode with the one of a helper setup mode (+0x190..+0x1a8: four qwords the mode loader 0x142a14f20 fills from
// weekend_options_0: the practice, qualifying and race groups and the length options they offer). It is an exchange,
// not a copy: each container stays owned by exactly one mode object, so a game rebuild that frees every mode while a
// swap is in place still frees each container once (forget). The swap is held for the whole weekend and undone by the
// next press of another box, a stock box or a picker build without an addon list. Only one host mode takes the
// helper's container: a second host mode keeps its own (one container cannot sit in two modes).
// The options tag (+0x00: the options_system game_setup_mode tag Sess_Parse_GameSetupMode resolves from the row's
// options_tag through 0x142a1a740 and stores at 0x142a187e4) decides which values the weekend menu offers
// (options_system.context tag restrictions); it is exchanged with the weekend options so the menu offers the helper's.
constexpr size_t MODE_OPTIONS_TAG = 0x00;
constexpr size_t MODE_WEEKEND_OPTIONS = 0x190;
constexpr size_t WEEKEND_OPTION_QWORDS = 5;
inline constexpr size_t WEEKEND_SWAP_OFFSETS[WEEKEND_OPTION_QWORDS] = {
    MODE_OPTIONS_TAG, MODE_WEEKEND_OPTIONS, MODE_WEEKEND_OPTIONS + 8, MODE_WEEKEND_OPTIONS + 16, MODE_WEEKEND_OPTIONS + 24};

class WeekendSwaps {
public:
    // Exchanges host's and helper's weekend options; a different held pair is put back first. False when host or
    // helper is missing or the two are the same object.
    bool swap(uint8_t *host, uint8_t *helper)
    {
        if (!host || !helper || host == helper)
            return false;
        if (held_ && held_->host == host && held_->helper == helper)
            return true;
        put_back();
        exchange(host, helper);
        Held h{host, helper, {}, {}};
        for (size_t i = 0; i < WEEKEND_OPTION_QWORDS; ++i) {
            h.host_now[i] = slots(host)[i];
            h.helper_now[i] = slots(helper)[i];
        }
        held_ = h;
        return true;
    }

    // Exchanges back when both modes still hold what the swap left in them; otherwise drops the pair without writing.
    size_t put_back()
    {
        if (!held_)
            return 0;
        Held h = *held_;
        held_.reset();
        for (size_t i = 0; i < WEEKEND_OPTION_QWORDS; ++i)
            if (slots(h.host)[i] != h.host_now[i] || slots(h.helper)[i] != h.helper_now[i])
                return 0;
        exchange(h.host, h.helper);
        return 1;
    }

    // Drops the pair without reading or writing the modes, which the game may have freed.
    size_t forget()
    {
        size_t n = held_ ? 1 : 0;
        held_.reset();
        return n;
    }

    size_t active() const { return held_ ? 1 : 0; }
    const uint8_t *helper() const { return held_ ? held_->helper : nullptr; }

private:
    struct Held {
        uint8_t *host, *helper;
        uint64_t host_now[WEEKEND_OPTION_QWORDS], helper_now[WEEKEND_OPTION_QWORDS];
    };
    struct Slots {
        uint8_t *mode;
        uint64_t &operator[](size_t i) const { return *reinterpret_cast<uint64_t *>(mode + WEEKEND_SWAP_OFFSETS[i]); }
    };
    static Slots slots(uint8_t *mode) { return {mode}; }
    static void exchange(uint8_t *a, uint8_t *b)
    {
        for (size_t i = 0; i < WEEKEND_OPTION_QWORDS; ++i)
            std::swap(slots(a)[i], slots(b)[i]);
    }
    std::optional<Held> held_;
};

}
