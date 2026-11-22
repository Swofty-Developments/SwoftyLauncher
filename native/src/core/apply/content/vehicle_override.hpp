#pragma once

// team.vehicle_override (experimental, SWOFTY_VEHICLE_OVERRIDE=1): one package team's car for chosen events,
// written into the session's own copy of the entrant records before the game builds the session's cars
// (vehicle_override.cpp, re/research/session_vehicle_swap.md).

namespace swofty_core::apply::content {

// Adds the team.vehicle_override applier (Applies::session); register_appliers calls it once.
void register_vehicle_override();
// The wrap at the start of Game_ChampionshipGameMode_BuildSessionSettings: writes the armed overrides into the
// current session's entrant copy, the same values at every fill of the session.
void vehicle_override_fill();
// Whether the wrap installed (the core set's commit).
void vehicle_override_wrap_live(bool live);
// career.saving: puts the team's car back in the session copy, so a save never holds an override's vehicle.
void vehicle_override_saving();
// career.saved: writes the session's overrides again after a save that put them back.
void vehicle_override_saved();

}
