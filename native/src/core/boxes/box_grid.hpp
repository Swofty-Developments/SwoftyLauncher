#pragma once

#include <optional>
#include <string>

#include "core/boxes/box_grid_logic.hpp"

namespace runtime {

// The pressed box's grid rule (none: a stock box, a box without a rule, or no box). Called by the era picker.
void box_grid_set(const std::optional<box_grid::Rule> &rule, const std::string &box);

}
