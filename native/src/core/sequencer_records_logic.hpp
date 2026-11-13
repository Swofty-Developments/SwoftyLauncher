#pragma once

#include <cstdint>

namespace runtime::sequencer_records {

inline bool skips(uint64_t first_match)
{
    return !first_match;
}

inline bool says(uint64_t first_match, bool &said)
{
    if (!skips(first_match) || said)
        return false;
    said = true;
    return true;
}

enum class Miss { unread, domain, no_table, past_end, empty_slot };

enum class Driver { team_mate, player };

inline const char *who(Driver d)
{
    return d == Driver::player ? "the player's own driver" : "the team mate's driver";
}

inline uint64_t driver_count(uint64_t begin, uint64_t end)
{
    return end > begin ? (end - begin) >> 4 : 0;
}

inline Miss miss(uint8_t domain, uint64_t table, uint64_t count, uint16_t id)
{
    if (domain > 2)
        return Miss::domain;
    if (!table)
        return Miss::no_table;
    if (id >= count)
        return Miss::past_end;
    return Miss::empty_slot;
}

inline const char *miss_text(Miss m)
{
    switch (m) {
    case Miss::domain:
        return "its domain is above 2";
    case Miss::no_table:
        return "the entrants system has no driver table for its domain";
    case Miss::past_end:
        return "its stat id is past the end of its domain's driver table";
    case Miss::empty_slot:
        return "its slot in its domain's driver table is empty";
    case Miss::unread:
        break;
    }
    return "its driver reference could not be read";
}

}
