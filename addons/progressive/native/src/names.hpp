// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Progressive Career: gamedb labels and stat ids of the rows an entrant record points at. The
// record keeps each as a key with the row's stat_id in the high word (swofty/game/entrants.hpp,
// Sess_Parse_Entrant 0x142452e60):
//   driver                 entrants.drivers.driver_N
//   team_stat_id           entrants.teams.team_N
//   vehicle_ref            entrants.vehicles.vehicle_N (Game_Lookup_VehicleStatIdByLabel_Thunk:
//                          {u8 0, u16 stat_id at +2})
//   driver_customisation   entrants.driver_customisations.folder_N.
//                          preloaded_driver_customisation_properties_M, the look (helmet, suit,
//                          gloves); stat_id << 16 as the career setup packs it (Career Mode Editor
//                          entrant_look, in game since 0.3.4)
// Built once per game database (names_logic.hpp) on a background thread through swofty/db.hpp;
// load() never waits: it starts the build and answers false until the snapshot is published,
// load_wait() joins it, refresh() rebuilds when the database fingerprint changed.

#pragma once
#include <cstdint>
#include <string>

namespace pg::names {

enum class Kind { driver, team, vehicle, look };

bool load();
bool load_wait();
void refresh();
bool loaded();
const char *kind_name(Kind k);
std::string label(Kind k, uint32_t stat);
bool stat(Kind k, const std::string &label, uint32_t &out);
bool known(Kind k, uint32_t stat);
size_t count(Kind k);
std::string key_text(Kind k, uint32_t key);

} // namespace pg::names
