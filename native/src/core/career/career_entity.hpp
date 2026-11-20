#pragma once

#include "core/career/career_state.hpp"

#include <cstdint>
#include <string>

// What the career library's flow states and API (career.cpp) and its reads of the championship entity
// (career_entity.cpp) share.
namespace career::game {

// The binding's address, resolved once (a miss before code ready is resolved again on the next call).
uintptr_t binding(const std::string &id);

// The championship entity, or 0 while none is loaded or its chain does not read.
uintptr_t entity();
// The entity's component of a k_Career_<type>Desc (or a full k_ desc id), or 0.
uintptr_t component(uintptr_t e, const char *type);
// The entity's component of that desc, or 0.
uintptr_t component_by_desc(uintptr_t e, uintptr_t desc);
// The loaded career's components, mode and human entrant.
career::state::Components poll();
// Hands the entity's entrants to the My Team vehicle heal.
void heal_my_team();

}
