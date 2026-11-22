#pragma once

#include "core/apply/content/box_options.hpp"
#include "core/apply/content/career_rules.hpp"
#include "core/apply/content/race_fuel.hpp"
#include "core/apply/content/setup_locks.hpp"

#include <cstdint>
#include "core/boxes/box_grid_logic.hpp"

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace swofty_core::apply::content {

void register_appliers();
// ui.per_package (box_ui.cpp): the box's own UI textures, routed while the box is pressed.
void register_box_ui();
// Starts the boot expansion ahead on a worker once the database load is hooked; SWOFTY_BOOT_AHEAD=0 turns it off.
void start_boot_ahead();
void undress_teams(const char *why);
// The team.identity id the loaded team record of this stat id wears in the current career (a dressed slot), or "".
std::string worn_team(uint16_t stat);

// The track codes whose folder in the game holds environment_package/tracks/<code>/wep/<code>_<name>.erp, the world a
// content.package names (box_world.hpp); nullopt while box worlds or SWOFTY_BOX_OWN_WORLDS are off.
std::optional<std::vector<std::string>> box_world_tracks(const std::string &name);

// The DRS group the box with this entrant list runs: the label of its content.package's rules.drs group if the
// boot apply added it (empty otherwise), and the line to log. host names the setup mode the rules go into in that line,
// here and in the other box lookups that take it.
struct DrsBox {
    std::string label, line;
};
DrsBox drs_for_box(const std::string &entrant_list, const std::string &host = "swofty_f1");

// The fuel rules of the box with this entrant list (race_fuel.hpp): its content.package's rules.refuelling,
// race_fuel_kg, refuel_stint_kg, quali_fuel_kg and race_start_fuel_from_quali, and the line to log.
FuelRules fuel_rules_for_box(const std::string &entrant_list);

// The tyre warmer setting the box with this entrant list runs: its content.package's rules.tyre_warmers (none when the
// package has no rule), and the line to log.
struct WarmersBox {
    std::optional<bool> warmers;
    std::string line;
};
WarmersBox tyre_warmers_for_box(const std::string &entrant_list, const std::string &host = "swofty_f1");

// The engine whose rev lights the box with this entrant list shows in equal-performance sessions (rev_lights.hpp): the
// entrants.engines label most of its content.package's vehicles name (empty when none does), and the line to log.
struct RevLightsBox {
    std::string engine, line;
};
RevLightsBox rev_lights_for_box(const std::string &entrant_list);

// The mandatory tyre change rule the box with this entrant list runs: its content.package's
// rules.mandatory_tyre_change (none when the package has no rule), and the line to log.
struct TyreChangeBox {
    std::optional<bool> required;
    std::string line;
};
TyreChangeBox tyre_change_for_box(const std::string &entrant_list, const std::string &host = "swofty_f1");

// The race time limit and weekend the box with this entrant list runs: its content.package's rules.race_time_limit_s
// and rules.weekend_mode (a setup mode name; empty when the package has none), and the line to log.
struct WeekendBox {
    std::optional<uint32_t> time_limit_s;
    std::string weekend_mode, line;
};
WeekendBox weekend_for_box(const std::string &entrant_list, const std::string &host = "swofty_f1");
// The option rules the box with this entrant list runs (box_options.hpp): its content.package's rules.safety_car,
// rules.formation_lap, rules.penalties and rules.corner_cutting (none where the package has no rule), and the line to
// log.
BoxOptionsPick box_options_for_box(const std::string &entrant_list);
// The setup sliders the box with this entrant list locks (setup_locks.hpp): its content.package's rules.setup_locked as
// lock mask bits (0 when the package has none), and the line to log.
SetupLocksPick setup_locks_for_box(const std::string &entrant_list);
// The grid rule the box with this entrant list runs: its content.package's rules.grid (none when the package has no
// rule), and the line to log. any_grid_rule: whether any loaded content.package has a rules.grid.
struct GridBox {
    std::optional<runtime::box_grid::Rule> rule;
    std::string line;
};
GridBox grid_for_box(const std::string &entrant_list);
bool any_grid_rule();
// The drivers of the content.package of the box with this entrant list (0 when it has none): how far past 22 cars the
// era picker raises its grid.
size_t seats_for_box(const std::string &entrant_list);
// The qualifying tyre rule the box with this entrant list runs: its content.package's rules.qualifying_tyres as a
// class option mask (bit 0 softer, 1 medium, 2 harder; 0 = no rule), and the line to log.
struct QualiTyresBox {
    uint8_t mask = 0;
    std::string line;
};
QualiTyresBox quali_tyres_for_box(const std::string &entrant_list);
// Whether any registered content.package carries a qualifying tyre rule (the core offers its tyre hooks only then).
bool any_quali_tyres();

// The points system the box with this entrant list runs: its content.package's rules.points system if the boot apply
// added it (found false otherwise) as the game holds it (26 places, the 75 / 50 / 25 % levels, the fastest lap rule
// and bonus), and the line to log.
struct PointsBox {
    bool found = false;
    std::string label;
    std::vector<int32_t> full;
    std::vector<std::vector<int32_t>> reduced;
    int32_t bonus = 0;
    uint8_t rule = 0;
    std::string line;
};
PointsBox points_for_box(const std::string &entrant_list, const std::string &host = "swofty_f1");

// The box of the loaded career's season (career_rules.hpp) in this context, read from the registry store; safety_car
// runs the season's rules.safety_car too (SWOFTY_CAREER_SAFETY_CAR=1).
CareerBox career_box_now(const registry::Context &ctx, bool safety_car = false);

}
