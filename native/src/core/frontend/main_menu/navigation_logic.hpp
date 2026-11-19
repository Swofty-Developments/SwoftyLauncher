#pragma once

#include <cstdint>
#include <string>

#include "core/frontend/name_hash.hpp"

namespace frontend::main_menu {

using frontend::name_hash::djb2;

constexpr const char *SHOWROOM_ROUTE = "swofty_showroom";
constexpr const char *CUSTOMISATION_ROUTE = "swofty_customisation";
constexpr const char *HOME_BACK_ROUTE = "swofty_home";
constexpr const char *BACK_HOME_FROM_CONTAINER = "swofty_back_home";
constexpr const char *TIME_TRIAL_ROUTE = "swofty_time_trial";
constexpr const char *GRAND_PRIX_ROUTE = "swofty_grand_prix";
constexpr const char *GRAND_PRIX_SAVES_ROUTE = "swofty_grand_prix_saves";
constexpr const char *STOCK_ACTIVITY_ROUTE = "home_to_activity";
constexpr const char *GRAND_PRIX_COMMAND = "solo_gp";
constexpr const char *FRONTEND_FLOW = "frontend_flow";
constexpr const char *F1_WORLD_FLOW = "f1_world_flow";
constexpr const char *CUSTOMISATION_FLOW = "customisation_flow";
constexpr const char *CUSTOMISATION_GROUP = "customisation_f1_life_entry";

enum class Mode { none, time_trial, grand_prix };

inline const char *press_route(Mode m)
{
    switch (m) {
    case Mode::time_trial:
        return TIME_TRIAL_ROUTE;
    case Mode::grand_prix:
        return STOCK_ACTIVITY_ROUTE;
    default:
        return "";
    }
}

// The Grand Prix card opens the activity container through the game's own home_to_activity (an entry with no command)
// and raises solo_gp on it, the command the container's own Grand Prix card raises. The game answers with
// grand_prix_resume, whose route migrates to the front-end flow's enter_gp_resume and opens grand_prix_continue: the
// saved seasons, Continue and New Grand Prix, as F1 World > Play > Grand Prix shows them; with no saved season the game
// opens the new Grand Prix picker itself.

inline const char *return_command(Mode m)
{
    switch (m) {
    case Mode::time_trial:
        return "time_trial";
    case Mode::grand_prix:
        return GRAND_PRIX_COMMAND;
    default:
        return "";
    }
}

// The card mode a session counts for, from the game mode of the session that loaded (apply::facts::mode_of names). The
// container a card opens keeps its own tabs, so a Grand Prix card press can end in a Time Trial (or the other way round):
// the session's mode then wins. Any other known mode has no card command; an unread or unknown mode keeps the press.
inline Mode driven_mode(Mode pressed, const std::string &session_mode)
{
    if (pressed == Mode::none || session_mode.empty() || session_mode == "unknown")
        return pressed;
    if (session_mode == "time_trial")
        return Mode::time_trial;
    if (session_mode == "grand_prix")
        return Mode::grand_prix;
    return Mode::none;
}

enum class Back { stock, route, home, twostep };

inline Back back_variant_of(const std::string &v, bool data_in_build)
{
    if (v == "stock")
        return Back::stock;
    if (v == "twostep")
        return Back::twostep;
    if (v == "home")
        return Back::home;
    return data_in_build ? Back::route : Back::stock;
}

// The route the launcher fires in place of the game's own Back route (nullptr: the game's stays). The container's
// Back fires f1_world (to the hub) or home; the Showroom's fires back_1.
inline const char *back_replacement(Back variant, uint32_t group, const std::string &route)
{
    if (variant == Back::route || variant == Back::home) {
        if (group == djb2("f1_world_activity_container") && (route == "f1_world" || route == "home"))
            return variant == Back::home ? "home" : BACK_HOME_FROM_CONTAINER;
    }
    if (variant == Back::route && route == "back_1" && group == djb2("fe_showroom"))
        return HOME_BACK_ROUTE;
    return nullptr;
}

// The Customisation screen's Back after a Customisation card press: back (what the screen fires when it was opened
// from the main menu; it leads nowhere there), back_1 (to the F1 World hub) or back_2 (to the news) becomes the
// launcher's swofty_back_home in that screen's group, the container's home route.
inline const char *customisation_back(Back variant, bool from_card, uint32_t group, const std::string &route)
{
    if ((variant == Back::route || variant == Back::home) && from_card && group == djb2(CUSTOMISATION_GROUP) &&
        (route == "back" || route == "back_1" || route == "back_2"))
        return BACK_HOME_FROM_CONTAINER;
    return nullptr;
}

// A Customisation card press stays armed until the front-end flow is entered again: the customisation screens' help
// page (team_hq_flow) migrates back into the customisation flow.
inline bool customisation_disarms(const std::string &flow) { return flow == FRONTEND_FLOW; }

// The saves list's Back (route back in grand_prix_continue) stays the game's: it migrates to the activity container,
// and swapping it for a front-end route home left a black screen. After a Grand Prix card press the core then leaves
// the container for the main menu once its tabs exist, so one Back from the saves list returns home.
constexpr uint64_t SAVES_BACK_DELAY_MS = 400, SAVES_BACK_TIMEOUT_MS = 6000;

inline bool saves_back_leaves_home(Back variant, uint32_t group, const std::string &route, Mode mode)
{
    return variant == Back::route && mode == Mode::grand_prix && group == djb2("grand_prix_continue") && route == "back";
}

enum class Expect { none, container, saves, home };

enum class Recovery { none, back_home_route, open_home };

// What to do when the screen a transition asked for did not open in time. flow_now is the flow the last migration
// the game accepted asked for; a main menu wanted while another flow is current needs the migration back.
inline Recovery recovery_for(Expect e, const std::string &flow_now, bool home_screen_open)
{
    if (e == Expect::none || (e == Expect::home && home_screen_open))
        return Recovery::none;
    if (flow_now == F1_WORLD_FLOW || flow_now == CUSTOMISATION_FLOW)
        return Recovery::back_home_route;
    return home_screen_open ? Recovery::none : Recovery::open_home;
}

// Whether the F1 World entry point (g_Game_F1WorldEntryPoint) may be cleared when a Back leaves for the main menu. The
// global is only written through the address the running build's binding set resolved: an unresolved binding (a
// build whose scan found no match) skips the write, as does an address that is not writable memory. The Steam offset
// is a read-only .sbss page on the EA build, and a write there faults.
enum class EntryPointWrite { write, unresolved, unwritable };

inline EntryPointWrite entry_point_write(uintptr_t resolved, bool writable)
{
    if (!resolved)
        return EntryPointWrite::unresolved;
    return writable ? EntryPointWrite::write : EntryPointWrite::unwritable;
}

inline bool migrate_failed(const std::string &text)
{
    return text.find("failed to migrate") != std::string::npos || text.find("No screen called") != std::string::npos;
}

// A Migrate command is logged when it fails or moves the tracked flow, not on every migrate.
inline bool migrate_logged(bool failed, const std::string &flow, const std::string &flow_before)
{
    return failed || (!flow.empty() && flow != flow_before);
}

// A session that ended after a Time Trial or Grand Prix card press hands over to the f1_world flow's container; the
// card's command is raised there once it is up.
inline bool return_due(bool was_in_session, bool in_session, Mode last, bool feature_on)
{
    return feature_on && was_in_session && !in_session && last != Mode::none;
}

}
