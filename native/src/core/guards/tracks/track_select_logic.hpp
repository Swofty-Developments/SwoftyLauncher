#pragma once

#include "core/apply/tracks/base.hpp"

#include <cstdint>
#include <string_view>

namespace runtime::track_select {

constexpr uint32_t ADVANCE_PRESSED = 0xfc8f026cu, SELECT_PRESSED = 0x2b0357au, SELECTED_CHANGED = 0xdcd54bcbu;

enum class Command { selection_changed, confirm, other };

// What a track select screen's command is: its selection changing, its confirm (`confirm`: the Grand Prix screen's
// advance_pressed or the Time Trial screen's select_pressed), or anything else, which the guard passes on.
constexpr Command command(uint32_t hash, uint32_t confirm)
{
    if (hash == SELECTED_CHANGED)
        return Command::selection_changed;
    return hash == confirm ? Command::confirm : Command::other;
}

// True when the game looked a track record up after the screen's last selection change, so a confirm is for that
// record's track. The lookup and the selection change number themselves from one counter, and 0 is neither.
constexpr bool looked_up(uintptr_t record, uint64_t record_seq, uint64_t select_seq) { return record && record_seq > select_seq; }

// The base track a looked-up record names: the one with its code, else the one with its stat id. None before the
// boot pass has filled the base.
inline const swofty_core::apply::tracks::BaseTrack *track_of(const swofty_core::apply::tracks::Base &base, std::string_view code,
                                                             uint8_t stat)
{
    if (!base.ready)
        return nullptr;
    const swofty_core::apply::tracks::BaseTrack *track = base.by_code(code);
    return track ? track : base.by_stat_id(stat);
}

}
