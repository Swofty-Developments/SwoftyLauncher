// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Progressive Career: when a weekend's entrants are rebuilt by the game (season.cpp
// refresh_weekends). Pure logic, no game access, so native/test builds it on the host.
//
// A weekend's first session holds a copy of the career rows with a car, made when the game built
// the weekend (0x140af7680 -> Weekend::start 0x140af0460). A season apply that changes how many
// rows have a car, or which drivers they are, leaves that copy behind, and the game only rebuilds
// it for a weekend still on its first session when its session layout changes, so it can keep the
// creation grid for the whole season. Weekend::start(weekend, entrants, begin = false) refills the
// current session from a list of career rows through the game's own pushes and rebuilds the
// weekend's entrant pointer list; it is called only for a weekend nobody has started:
//   progress 0 (Weekend.progress_state), at its first session (Weekend.current_session 0), with
//   sessions, its first session not begun (Session.completion 0) and holding a copy already;
// and only when that copy's drivers differ from the career rows with a car (as a multiset).

#pragma once
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <vector>

namespace progressive::weekend {

struct State {
    uint32_t progress = 0;
    uint32_t current = 0;
    size_t sessions = 0;
    uint32_t first_completion = 0;
    size_t first_rows = 0;
};

inline const char *not_rebuildable(const State &s)
{
    if (s.progress != 0)
        return "the weekend has started";
    if (s.sessions == 0)
        return "the weekend has no sessions yet";
    if (s.current != 0)
        return "the weekend is past its first session";
    if (s.first_completion != 0)
        return "its first session has begun";
    if (s.first_rows == 0)
        return "its first session holds no entrants yet";
    return nullptr;
}

inline bool same_entrants(std::vector<uint32_t> copy, std::vector<uint32_t> career)
{
    if (copy.size() != career.size())
        return false;
    std::sort(copy.begin(), copy.end());
    std::sort(career.begin(), career.end());
    return copy == career;
}

// Session.session_id values (the game's session enum): 1..4 practice, 5..9 qualifying, 10..14 sprint shootout,
// 15..17 races. Every weekend format lists its practice sessions first (a sprint weekend has one), so a practice
// session after a qualifying, shootout or race session is a layout no format makes: the game keeps the completed
// sessions of the old layout first and appends the rest of the new one when a weekend's format changes under it.
// Returns the index of the first such practice session, or -1.
inline int impossible_order(const std::vector<uint32_t> &ids)
{
    bool later = false;
    for (size_t i = 0; i < ids.size(); ++i) {
        bool practice = ids[i] >= 1 && ids[i] <= 4;
        if (practice && later)
            return static_cast<int>(i);
        if (ids[i] >= 5)
            later = true;
    }
    return -1;
}

} // namespace progressive::weekend
