#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace runtime::direct_start {

constexpr uint32_t STATE_FRONTEND_WORLD = 0x290b11ae;
constexpr uint32_t STATE_INTRO = 0xa4d3a4b2;
constexpr uint32_t STATE_FTUE = 0x04fc8ffa;
constexpr uint32_t STATE_ONLINE_FRONTEND = 0x5f9ae18b;
constexpr uint32_t LINK_STAY = 0;
constexpr uint32_t LINK_EXIT = 0x0eef86be;
constexpr uint32_t LINK_SESSION = 0x8c23ce89;

enum class Phase : uint8_t { off, profile, setup, leave, left, failed };

inline const char *phase_name(Phase p)
{
    switch (p) {
    case Phase::off:
        return "off";
    case Phase::profile:
        return "waiting for the profile load";
    case Phase::setup:
        return "setting up the target";
    case Phase::leave:
        return "leaving the front end";
    case Phase::left:
        return "left the front end";
    case Phase::failed:
        return "failed";
    }
    return "?";
}

enum class Setup : uint8_t { pending, done, failed };

// A target holds `exit_state`'s finished link until the target is set up. A session target then replaces it
// with the exit link, so the menu states never start, and forces `link` from `from_state` once that has no
// child. A room target then lets it through: home starts and takes the lobby link itself.
struct Target {
    std::string name;
    uint32_t exit_state = STATE_FTUE;
    uint32_t from_state = STATE_FRONTEND_WORLD;
    uint32_t link = LINK_SESSION;
};

inline Target session_target() { return {"session", STATE_FTUE, STATE_FRONTEND_WORLD, LINK_SESSION}; }

inline Target room_target() { return {"room", STATE_FTUE, 0, 0}; }

struct Driver {
    Phase phase = Phase::off;
    Target target;
    std::string why;
    bool exited = false;

    void arm(const Target &t)
    {
        target = t;
        why.clear();
        exited = false;
        phase = Phase::profile;
    }

    bool engage_title() const { return phase == Phase::profile || phase == Phase::setup || phase == Phase::leave; }

    void profile_loaded()
    {
        if (phase == Phase::profile)
            phase = Phase::setup;
    }

    void setup_result(Setup s, const std::string &reason = {})
    {
        if (phase != Phase::setup)
            return;
        if (s == Setup::done)
            phase = target.from_state ? Phase::leave : Phase::left;
        else if (s == Setup::failed) {
            phase = Phase::failed;
            why = reason;
        }
    }

    uint32_t on_update(uint32_t state, uint32_t link, bool has_child)
    {
        bool waiting = phase == Phase::profile || phase == Phase::setup;
        if (target.exit_state && state == target.exit_state && link != LINK_STAY && !exited) {
            if (waiting)
                return LINK_STAY;
            if (phase == Phase::leave) {
                exited = true;
                return LINK_EXIT;
            }
            return link;
        }
        if (phase == Phase::leave && state == target.from_state && !has_child && (exited || !target.exit_state)) {
            phase = Phase::left;
            return target.link;
        }
        return link;
    }
};

// The flow link hook has work while the start drives the flow (profile, setup, leave) and, for a session target, until the
// session's screen is up or the cover's wait ran out. After that, and after a failed start, it returns at once.
inline bool link_needed(Phase phase, bool session_target, bool session_screen, uint64_t session_at, uint64_t now,
                        uint64_t screen_wait_ms, bool diag)
{
    if (diag)
        return true;
    if (phase == Phase::off || phase == Phase::failed)
        return false;
    if (phase != Phase::left)
        return true;
    if (!session_target || session_screen)
        return false;
    return !session_at || now < session_at || now - session_at < screen_wait_ms;
}

// A game setup mode of the Grand Prix mode, by name, and whether one of its entrant setup modes offers the
// requested entrant list.
struct SetupModeOffer {
    std::string name;
    bool offers = false;
};

constexpr size_t NO_SETUP_MODE = size_t(-1);

// The game setup mode a Grand Prix direct start takes. A requested name is taken as it is. Otherwise the host mode
// when it offers the list, then the first mode that does, since the session's track data query pairs the setup mode
// with the list's season and finds no track for a pair no track data set holds (swofty_f1 with the 2026 list);
// the host mode when no mode offers the list.
inline size_t pick_setup_mode(const std::vector<SetupModeOffer> &modes, const std::string &requested, const std::string &host)
{
    auto named = [&](const std::string &name) {
        for (size_t i = 0; i < modes.size(); i++)
            if (modes[i].name == name)
                return i;
        return NO_SETUP_MODE;
    };
    if (!requested.empty())
        return named(requested);
    size_t h = named(host);
    if (h != NO_SETUP_MODE && modes[h].offers)
        return h;
    for (size_t i = 0; i < modes.size(); i++)
        if (modes[i].offers)
            return i;
    return h;
}

constexpr size_t NO_LIST = size_t(-1);

// The entrant list a Grand Prix direct start names, among the names the entrants system holds. The game's list
// lookup (0x1424286e0, name to stat id) builds the path "Invalid.entrant_lists" for a name it has no entry for and
// then dereferences the garbage it reads there, so a name is looked up only once it is known to be held.
inline size_t find_list(const std::vector<std::string> &names, const std::string &want)
{
    if (want.empty())
        return NO_LIST;
    for (size_t i = 0; i < names.size(); i++)
        if (names[i] == want)
            return i;
    return NO_LIST;
}

inline std::string unknown_list_why(const std::vector<std::string> &names, const std::string &want, size_t shown = 40)
{
    std::string why = "no entrant list '" + want + "' (a box is its entrant list's name, not its package's; lists:";
    size_t n = 0;
    for (const auto &name : names)
        if (!name.empty() && n++ < shown)
            why += " " + name;
    if (n > shown)
        why += " and " + std::to_string(n - shown) + " more";
    return why + ")";
}

}
