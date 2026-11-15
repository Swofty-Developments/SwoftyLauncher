#pragma once

#include "race_fuel.hpp"

#include <cstdint>
#include <optional>
#include <string>

namespace swofty_core::apply::content {

void register_appliers();
void undress_teams(const char *why);

// The DRS group the box with this entrant list runs: the label of its content.package's rules.drs group if the
// boot apply added it (empty otherwise), and the line to log.
struct DrsBox {
    std::string label, line;
};
DrsBox drs_for_box(const std::string &entrant_list);

// The fuel rules of the box with this entrant list (race_fuel.hpp): its content.package's rules.refuelling,
// race_fuel_kg, refuel_stint_kg, quali_fuel_kg and race_start_fuel_from_quali, and the line to log.
FuelRules fuel_rules_for_box(const std::string &entrant_list);

// The tyre warmer setting the box with this entrant list runs: its content.package's rules.tyre_warmers (none when the
// package has no rule), and the line to log.
struct WarmersBox {
    std::optional<bool> warmers;
    std::string line;
};
WarmersBox tyre_warmers_for_box(const std::string &entrant_list);

// The race time limit and weekend the box with this entrant list runs: its content.package's rules.race_time_limit_s
// and rules.weekend_mode (a setup mode name; empty when the package has none), and the line to log.
struct WeekendBox {
    std::optional<uint32_t> time_limit_s;
    std::string weekend_mode, line;
};
WeekendBox weekend_for_box(const std::string &entrant_list);

}
