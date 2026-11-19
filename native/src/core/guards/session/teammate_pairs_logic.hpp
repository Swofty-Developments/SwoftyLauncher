#pragma once
#include "core/guards/garage/garage_crew_logic.hpp"
#include "core/guards/skips.hpp"

#include <cstdint>

namespace runtime::teammate_pairs {

constexpr uint64_t CAR_BYTES = 8;
constexpr uint64_t PAIR_BYTES = garage_crew::CREW_SLOTS * CAR_BYTES;
constexpr uint64_t MAX_BYTES = 0x800;
constexpr uint64_t TEAM_ID = 0x28;
constexpr size_t SEEN_TEAMS = 64;

inline bool over(uint64_t bytes) { return bytes > PAIR_BYTES && bytes <= MAX_BYTES && bytes % CAR_BYTES == 0; }

inline uint64_t cars(uint64_t bytes) { return bytes / CAR_BYTES; }

// The teams the guard has logged in the current session: each of the session's first SEEN_TEAMS distinct teams once.
struct Seen {
    uint64_t session = 0;
    swofty_core::guards::FirstKeys<uint32_t, SEEN_TEAMS> teams;

    bool first(uint64_t now, uint32_t team)
    {
        if (now != session) {
            session = now;
            teams.clear();
        }
        return bool(teams.note(team));
    }
};

}
