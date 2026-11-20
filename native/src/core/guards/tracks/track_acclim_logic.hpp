#pragma once

#include <cstddef>
#include <cstdint>

namespace runtime::track_acclim {

using Read = bool (*)(uint64_t address, void *out, size_t n, void *user);

// The programme's corner targets: a vector of 0x370-byte entries.
constexpr size_t TARGETS_BEGIN = 0x350;
constexpr size_t TARGETS_END = 0x358;

// True when the Track Acclimatisation programme at `programme` has no targets: both ends of its target vector read and
// are equal. A programme whose vector can't be read runs the game's code. `begin` gets the vector's start for the line.
inline bool no_targets(uint64_t programme, Read read, void *user, uint64_t &begin)
{
    uint64_t end = 0;
    begin = 0;
    return read(programme + TARGETS_BEGIN, &begin, sizeof begin, user) && read(programme + TARGETS_END, &end, sizeof end, user) &&
           begin == end;
}

}
