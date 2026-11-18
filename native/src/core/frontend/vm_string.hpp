#pragma once
#include <swofty/game.hpp>

namespace frontend::vm {

// Writes a string leaf of a view model node: makes the engine string, sets it (overwriting) and lets go of it.
inline void set_string(void *reg, void *parent, const char *name, const char *text)
{
    swofty::game::NeStr *s = nullptr;
    swofty::game::Ne_Make_StrFromCStr(&s, text, 0, 0);
    swofty::game::Vm_Set_String(reg, parent, name, &s, 1);
    swofty::game::Ne_Make_StrFromCStr(&s, nullptr, 0, 0);
}

}
