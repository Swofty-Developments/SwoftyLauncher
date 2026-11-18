#pragma once

#include "core/apply/content/race_fuel.hpp"

namespace swofty_core::hooks {

// The pressed box's fuel rules (apply/content/race_fuel.hpp) for car_live.cpp to play in the race; the era picker sets
// them on each press and clears them with empty rules.
void race_fuel_rules(const apply::content::FuelRules &rules);

}
