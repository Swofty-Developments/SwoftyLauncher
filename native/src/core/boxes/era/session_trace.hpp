#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace runtime::era {

// Session trace (diagnostic, read only). A Time Trial after a Grand Prix on a 24-car box spawns 27 vehicles
// while the entrants manager's session entrants (EntrantsManager +0x2e0..+0x2e8, 0x88-byte records, team stat
// id u16 at +0x5a) hold at most 23 at StateLoadSession Entering. The Time Trial mode's descriptor fill
// (0x142936760, vtable 0x1457b13d0 slot 0x80) clears those entrants, pushes its own and builds one 0x6780-byte
// session participant per entrant with a vehicle into the descriptor (+0xad0 vector, begin +0xad8, end +0xae0).
// The trace logs, at each StateLoadSession step, at StateSession Entering and at the session vehicle setup,
// the manager's entrant count and team ids, its list and setup mode, the participant counts of the session
// descriptors the game manager (+0x1a8) and game module (game manager +0x140, its +0x1a8 and +0x198) hold, and
// the session module's vehicle count, so one run shows which of them holds the 27 and when it fills.
constexpr size_t MGR_SESSION_BEGIN = 0x2e0;
constexpr size_t MGR_SESSION_END = 0x2e8;
constexpr size_t ENTRANT_DRIVER = 0x52;
constexpr size_t ENTRANT_TEAM = 0x5a;
constexpr size_t ENTRANT_VEHICLE_ID = 0x5e;
constexpr size_t PARTICIPANT = 0x6780;
constexpr size_t DESC_PARTICIPANTS_BEGIN = 0xad8;
constexpr size_t DESC_PARTICIPANTS_END = 0xae0;
constexpr size_t GM_SESSION = 0x1a8;
constexpr size_t GM_MODULE = 0x140;
constexpr size_t MODULE_SESSION = 0x1a8;
constexpr size_t MODULE_SESSION_NEXT = 0x198;
constexpr size_t MAX_PARTICIPANTS = 64;
constexpr uint32_t TRACE_LINES = 120;

inline bool trace_flow(std::string_view state, int to)
{
    return ((state == "StateLoadSession" || state == "StateUnloadSession") && to >= 1 && to <= 3) ||
           (state == "StateSession" && (to == 1 || to == 3));
}

// The Time Trial cut stays through StateSession onStateExit and StateUnloadSession onStateEnter, whose loader keep
// checks (0x1436f8340, 0x143709970 -> 0x142d48040) fill a temporary descriptor and compare it with the live one; it
// comes off at StateUnloadSession Exiting.
inline bool time_trial_cut_off(std::string_view state, int to) { return state == "StateUnloadSession" && to == 3; }

inline int64_t participant_count(uintptr_t begin, uintptr_t end)
{
    if (!begin || end < begin || (end - begin) % PARTICIPANT || (end - begin) / PARTICIPANT > MAX_PARTICIPANTS)
        return -1;
    return int64_t((end - begin) / PARTICIPANT);
}

struct SessionTrace {
    std::string where, mode, list, setup_mode;
    int64_t entrants = -1, list_cars = -1;
    std::vector<uint16_t> teams;
    int64_t gm_session = -1, module_session = -1, module_next = -1;
    int64_t vehicles = -1;
    int64_t budget = -1;
    int64_t previous = -1;
};

// The Time Trial budget's expected vehicles (its cars plus the game's 4 entries) against what the fill made: the
// participants in the game manager's descriptor, or the vehicles at the session's vehicle setup.
// The fill runs between StateLoadSession Updating and Exiting, so at Entering and Updating the descriptor may still
// hold the previous session's participants: there a count is compared only when it changed since the previous
// traced step (previous), which a fill before Entering would show.
inline std::string budget_mismatch(const SessionTrace &t)
{
    int64_t seen = t.vehicles >= 0 ? t.vehicles : t.gm_session;
    if (t.budget < 0 || seen <= 0 || seen == t.budget)
        return {};
    bool before_fill = t.where == "StateLoadSession Entering" || t.where == "StateLoadSession Updating";
    if (before_fill && t.vehicles < 0 && t.gm_session == t.previous)
        return {};
    return "Time Trial fill at " + t.where + " made " + std::to_string(seen) + " vehicles where the budget expected " +
           std::to_string(t.budget);
}

inline std::string count_text(int64_t n) { return n < 0 ? std::string("-") : std::to_string(n); }

inline std::string session_trace_line(const SessionTrace &t)
{
    std::string teams;
    for (size_t i = 0; i < t.teams.size(); ++i)
        teams += (i ? "," : "") + std::to_string(t.teams[i]);
    return "session trace " + t.where + " (" + (t.mode.empty() ? "?" : t.mode) + "): entrants " + count_text(t.entrants) +
           (teams.empty() ? "" : " (teams " + teams + ")") + ", list " + (t.list.empty() ? "?" : t.list) + " (" +
           count_text(t.list_cars) + " cars) of setup mode " + (t.setup_mode.empty() ? "?" : t.setup_mode) +
           "; participants: game manager " + count_text(t.gm_session) + ", game module " + count_text(t.module_session) +
           " (next " + count_text(t.module_next) + "); session vehicles " + count_text(t.vehicles) +
           (t.budget >= 0 ? "; Time Trial budget " + std::to_string(t.budget) : std::string());
}

}
