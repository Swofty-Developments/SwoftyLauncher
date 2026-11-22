#pragma once

#include "core/apply/content/race_fuel.hpp"

#include <cstdint>
#include <string>

namespace swofty_core::hooks {

// The pressed box's fuel rules (apply/content/race_fuel.hpp) for car_live.cpp to play in the race; the era picker sets
// them on each press and clears them with empty rules.
void race_fuel_rules(const apply::content::FuelRules &rules);

// The pressed box's setup lock bits (apply/content/setup_locks.hpp) for car_live.cpp to add to every setup entry's lock
// mask after each setup apply; the era picker sets them with the fuel rules and clears them with 0.
void box_setup_locks(uint64_t mask, const std::string &line);

}
