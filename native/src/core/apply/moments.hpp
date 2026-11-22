#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include "core/apply/facts.hpp"
#include "core/apply/ids.hpp"
#include "core/apply/room.hpp"

namespace swofty_core::apply::moments {

void attach();
void start_boot_plan();
bool boot_may_work();
// The database load is hooked, so the boot will run inside it: its content expansion starts ahead on a worker.
void boot_ahead();
void boot_load(room::Report &report);
void boot_without_database(room::Report report);
void write_room(const room::Report &report);
void status(const std::string &addon, const std::string &line);
void left_out(const std::string &addon, int teams, int vehicles, int lists);

void career_loaded(const char *key, unsigned mode);
void career_unloaded();
void career_season(int year);
void career_dated(int year);
void career_renamed(const char *from, const char *to);
void box_picked(const char *entrant_list);
void box_picked(const char *entrant_list, const void *mode_context);
void session_load(const char *source);
// StateSession is updating: the loaded session's facts take the weekend's current session again, for a game that moves
// the weekend on to the session that starts only after that session's start.
void session_running();
std::pair<std::string, std::string> session_mode_track();
// The current championship weekend of the loaded game mode, read live: round and rounds 1-based, the weekend's
// progress state (0 not started, 1 in progress, 2 complete), the current session's index, the session count and how
// many sessions are complete, the current session's name and the track (the tracks table only once it was read in
// this run). False when the chain does not read.
struct CareerWeekend {
    uint32_t round = 0, rounds = 0, progress = 0, current = 0, sessions = 0, complete = 0;
    std::string session, track;
};
bool career_weekend(CareerWeekend &out);
// The current championship session's own copy of the entrant records, read live: the session (weekend +0x220 +
// current index (+0x238) * 0xa0) and its record vector (Session +0x20 .. +0x28, 0x88 per record), at most 64 records.
// False when the chain does not read (team.vehicle_override, vehicle_override.cpp).
struct SessionEntrants {
    uintptr_t session = 0, begin = 0, end = 0;
    uint32_t session_id = 0;
};
bool session_entrants(SessionEntrants &out);
// Whether this game start is a shared session's (a direct start that targets a room).
bool room_boot();
// The kind of game mode the game is in: kept by a watch on its own thread (started by mode_watch) and by every session load.
facts::ModeKind mode_kind();
void mode_watch();
void grand_prix_track_check();
std::string current_mode(const char *where);
// The tracks table label of a stat id once the table was read in this run, else empty; never reads the database.
std::string cached_track_label(int stat_id);
void session_end(const char *source);
void window();
int write_reports(const char *when);
void crash_reports();

ids::CareerMap map_for_career(const std::string &career);
std::vector<ids::Clash> adopt(const std::string &career, const ids::CareerMap &map);

}
