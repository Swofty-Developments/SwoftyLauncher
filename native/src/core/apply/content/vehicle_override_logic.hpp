#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

// Pure logic of team.vehicle_override (vehicle_override.cpp), which the host test covers
// (native/test/vehicle_override_test.cpp).
//
// A session's own copy of the entrant records (Session +0x20..+0x28, 0x88 each; re/research/session_vehicle_swap.md)
// carries each entrant's vehicle ref at +0x5c ((stat id << 16) | kind 0) and its vehicle customisation (livery preset)
// key at +0x64 (high word 0x3fff: none, the vehicle's own livery). Game_ChampionshipGameMode_BuildSessionSettings
// resolves the car of every entrant from that copy at each session load. An override changes the seats of one package
// team: an entrant is the team's when its vehicle ref names the team's own car, and a seat is its team position
// (+0x80), which the core writes as the seat's index in the package team's drivers list. The record later in load
// order wins a seat two overrides name, and the plan says so. A write keeps what it found; it is put back only while
// the record still holds what the core wrote.

namespace swofty_core::apply::content::vehicle_override {

inline constexpr uint32_t NO_KEY = 0x3fff0000u;
inline constexpr int64_t STAT_MAX = 16382; // vehicle ids live in 14 bits where the game packs one (lower.hpp)
inline constexpr std::size_t RECORD_SIZE = 0x88, MAX_ENTRANTS = 64;
inline constexpr std::size_t VEHICLE_REF = 0x5c, CUSTOMISATION = 0x64, TEAM_POSITION = 0x80;

inline constexpr uint32_t ref_of(int64_t stat) { return uint32_t(stat & 0xffff) << 16; }
inline constexpr int64_t stat_of_ref(uint32_t ref) { return int64_t(ref >> 16); }
inline constexpr bool valid_stat(int64_t stat) { return stat >= 0 && stat <= STAT_MAX; }

// SWOFTY_VEHICLE_OVERRIDE: on for "1", "on" and "true"; anything else, or no variable, leaves the feature off.
inline bool enabled(std::string_view v) { return v == "1" || v == "on" || v == "true"; }

// The session an override is checked against: the apply context at the session load.
struct Session {
    std::string mode, type, track;
    int year = 0;
    bool has_year = false;
};

struct When {
    std::string track;
    int year = 0;
    std::vector<std::string> session_types;
};

inline bool when_matches(const When &w, const Session &s)
{
    if (!w.track.empty() && w.track != s.track)
        return false;
    if (w.year != 0 && (!s.has_year || s.year != w.year))
        return false;
    if (w.session_types.empty())
        return true;
    for (const std::string &t : w.session_types)
        if (t == s.type)
            return true;
    return false;
}

// Why the overrides do not run in this game mode, or empty when they do: Grand Prix, and the two careers while the
// save splice fires career.saving (so a save never stores an override). Time Trial fills its cars elsewhere; online
// modes (an "unknown" mode, as facts::mode_of maps every *_online label) and a shared session would desync the other
// players' copies; the other modes are not checked.
inline std::string mode_refusal(const std::string &mode, bool room, const std::string &save_blocked)
{
    if (room)
        return "this is a shared session (room): a car only this game swaps would differ from the other players' cars";
    if (mode == "time_trial")
        return "Time Trial fills its cars from whole lists of its own (Sess_TimeTrialMode_FillDescriptor)";
    if (mode == "grand_prix")
        return {};
    if (mode == "my_team" || mode == "driver_career") {
        if (!save_blocked.empty())
            return "a career needs the save splice's career.saving to put the team's car back before each save (" + save_blocked +
                   ")";
        return {};
    }
    return "game mode " + (mode.empty() ? std::string("unknown") : mode) +
           " is not Grand Prix or a career (online modes are refused)";
}

// The seats (team positions) of a package team that the override's drivers name: each driver's index in the team's
// drivers list. No drivers: every seat (an empty list). A driver the team does not name is an error in `why`.
inline bool seats_of(const std::vector<std::string> &team_drivers, const std::vector<std::string> &wanted,
                     std::vector<int> &seats, std::string &why)
{
    seats.clear();
    for (const std::string &d : wanted) {
        int at = -1;
        for (std::size_t i = 0; i < team_drivers.size(); i++)
            if (team_drivers[i] == d) {
                at = int(i);
                break;
            }
        if (at < 0) {
            why = "the team names no seat for driver " + d;
            return false;
        }
        bool have = false;
        for (int s : seats)
            have = have || s == at;
        if (!have)
            seats.push_back(at);
    }
    return true;
}

// One override resolved against the store, armed for the session.
struct Target {
    std::string id;
    std::size_t order = 0;     // load order: a higher one wins a seat
    int64_t team_vehicle = -1; // the stat id of the package team's own car
    std::vector<int> seats;    // empty: every seat
    uint32_t vehicle_ref = 0;  // ref_of(the override vehicle's stat id)
    uint32_t livery_key = NO_KEY;
};

// One record of the session's entrant copy, as read.
struct Entrant {
    uintptr_t at = 0;
    uint32_t vehicle_ref = 0, customisation = 0;
    uint16_t team_position = 0;
};

struct Write {
    uintptr_t at = 0;
    uint32_t vehicle_before = 0, vehicle_after = 0, livery_before = 0, livery_after = 0;
    std::string id;
};

struct Plan {
    std::vector<Write> writes; // the new writes; a record already holding a kept write is left as it is
    std::size_t kept = 0;      // records that still hold a write made at an earlier fill of this session
    std::vector<std::string> lines;
};

inline bool seat_matches(const Target &t, uint16_t position)
{
    if (t.seats.empty())
        return true;
    for (int s : t.seats)
        if (s == int(position))
            return true;
    return false;
}

inline std::string hex32(uint32_t v)
{
    static const char digits[] = "0123456789abcdef";
    std::string s = "0x";
    for (int i = 7; i >= 0; i--)
        s += digits[(v >> (i * 4)) & 0xf];
    return s;
}

// What one fill writes. A record that holds a kept write (same address, still the written values) is counted as kept,
// so every fill of a session gives the same answer and the loader's keep checks see the same cars.
inline Plan plan(const std::vector<Target> &targets, const std::vector<Entrant> &entrants, const std::vector<Write> &kept)
{
    Plan p;
    for (const Entrant &e : entrants) {
        bool held = false;
        for (const Write &w : kept)
            if (w.at == e.at && w.vehicle_after == e.vehicle_ref && w.livery_after == e.customisation) {
                held = true;
                break;
            }
        if (held) {
            p.kept++;
            continue;
        }
        const Target *win = nullptr;
        std::vector<const Target *> all;
        for (const Target &t : targets)
            if (stat_of_ref(e.vehicle_ref) == t.team_vehicle && seat_matches(t, e.team_position)) {
                all.push_back(&t);
                if (!win || t.order >= win->order)
                    win = &t;
            }
        if (!win)
            continue;
        if (all.size() > 1) {
            std::string names;
            for (const Target *t : all)
                if (t != win)
                    names += (names.empty() ? "" : ", ") + t->id;
            p.lines.push_back("seat " + std::to_string(e.team_position) + " of the car with stat id " +
                              std::to_string(stat_of_ref(e.vehicle_ref)) + ": " + win->id + " wins (later in load order) over " +
                              names);
        }
        if (win->vehicle_ref == e.vehicle_ref && win->livery_key == e.customisation)
            continue;
        p.writes.push_back({e.at, e.vehicle_ref, win->vehicle_ref, e.customisation, win->livery_key, win->id});
    }
    return p;
}

// Whether a kept write is put back: only while the record still holds what the core wrote.
inline bool restorable(const Write &w, uint32_t vehicle_now, uint32_t livery_now)
{
    return vehicle_now == w.vehicle_after && livery_now == w.livery_after;
}

}
