#pragma once

#include <string>

#include "direct_start_logic.hpp"
#include "json.hpp"

namespace runtime::direct_start::tt {

// The Time Trial for a session request with "mode": "time_trial" ({box, track, team, setup_mode}), polled on
// the flow thread while StateFTUE is held, until it answers done or failed.
Setup setup(const json::Value &request, std::string &why);

// Starts the setup over for a saved replay's session: the same steps, without the direct start's scene changes.
void reset_for_replay();

std::string current_track();

std::string session_track();
// The stat id byte of the Time Trial's track record (what a save slot stores as its track), or -1.
int track_stat();

// The entrant list the Time Trial mode has selected (the request's "box").
// The Time Trial game mode object and its entity, or nulls.
void mode_objects(void *&mode, void *&entity);
std::string player_list();

std::string player_team();

}
