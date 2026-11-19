#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <map>
#include <string>
#include <utility>
#include <vector>

#include "core/apply/tracks/base.hpp"
#include "core/apply/tracks/seed.hpp"
#include "core/runtime/json.hpp"

namespace runtime::era {

// Grand Prix load guard. Sess_Setup_ChampionshipEntrants (0x140ba3ad0) sets the entrants manager's list (+0xe18)
// only when the weekend's entrant setup mode offers exactly one list; swofty_f1 hosts every season box, so a Grand
// Prix > Continue lands at StateLoadSession with +0xe18 null and Sess_Assign_GarageSlotsByClass (0x1424a8cf0) reads
// [0+0x48]. At StateLoadSession Entering the core picks the setup mode's list that holds every team of the saved
// entrants (manager +0x2e0..+0x2e8, team stat id u16 at +0x5a of each 0x88-byte record, the same records a list
// holds from +0x10 to +0x18), else the list of the last box pressed. A saved weekend whose track is no track of the
// base (a dormant row whose addon is off: no track archive, no facts entry) would crash in
// Sess_Loader_SetupTrackAssets. With no list or no track the load is stopped: the core logs why and closes F1 25
// with CONTINUE_STOP_EXIT before the game touches either, and the launcher names the reason.
constexpr uint16_t NO_TEAM = 0x3fff;
constexpr unsigned CONTINUE_STOP_EXIT = 92;

struct ListTeams {
    std::string name;
    std::vector<uint16_t> teams;
};

struct ContinuePick {
    int64_t index = -1;
    std::string line;
    bool holds = false;
};

struct IdAliases {
    std::map<uint16_t, uint16_t> teams, vehicles, drivers;

    bool any() const { return !teams.empty() || !vehicles.empty() || !drivers.empty(); }
};

inline bool parse_aliases(const std::string &text, IdAliases &out)
{
    json::Value v;
    if (!json::parse(text, v) || v.type != json::Value::Obj || v["version"].num() != 1)
        return false;
    IdAliases a;
    auto table = [&](const char *name, std::map<uint16_t, uint16_t> &into) {
        for (auto &[k, to] : v["tables"][name].o) {
            long long from = strtoll(k.c_str(), nullptr, 10), now = to.num(-1);
            if (from > 0 && from < NO_TEAM && now > 0 && now < NO_TEAM && from != now)
                into[uint16_t(from)] = uint16_t(now);
        }
    };
    table("teams", a.teams);
    table("vehicles", a.vehicles);
    table("drivers", a.drivers);
    out = std::move(a);
    return true;
}

inline uint16_t alias_of(const std::map<uint16_t, uint16_t> &map, uint16_t id)
{
    auto it = map.find(id);
    return it == map.end() ? id : it->second;
}

inline std::vector<uint16_t> remap_teams(const IdAliases &aliases, std::vector<uint16_t> teams)
{
    for (uint16_t &t : teams)
        t = alias_of(aliases.teams, t);
    return teams;
}

inline bool saved_predates(const IdAliases &aliases, const std::vector<uint16_t> &teams)
{
    for (uint16_t t : teams)
        if (t && t != NO_TEAM && aliases.teams.count(t))
            return true;
    return false;
}

inline bool saved_predates(const IdAliases &aliases, const std::vector<uint16_t> &teams, const std::vector<uint16_t> &vehicles,
                           const std::vector<uint16_t> &drivers)
{
    if (saved_predates(aliases, teams))
        return true;
    for (uint16_t v : vehicles)
        if (v && v != NO_TEAM && aliases.vehicles.count(v))
            return true;
    for (uint16_t d : drivers)
        if (d && d != NO_TEAM && aliases.drivers.count(d))
            return true;
    return false;
}

inline std::vector<uint16_t> distinct_teams(const std::vector<uint16_t> &teams)
{
    std::vector<uint16_t> out;
    for (uint16_t t : teams)
        if (t && t != NO_TEAM && std::find(out.begin(), out.end(), t) == out.end())
            out.push_back(t);
    std::sort(out.begin(), out.end());
    return out;
}

inline std::string team_text(const std::vector<uint16_t> &teams)
{
    std::string s;
    for (size_t i = 0; i < teams.size(); ++i)
        s += (i ? "," : "") + std::to_string(teams[i]);
    return s.empty() ? std::string("none") : s;
}

inline ContinuePick continue_list(const std::vector<ListTeams> &lists, const std::vector<uint16_t> &entrants,
                                  const std::string &pressed, const std::string &setup_mode)
{
    ContinuePick p;
    std::vector<uint16_t> want = distinct_teams(entrants);
    std::string head = "the game left the entrants manager's list empty (setup mode " +
                       (setup_mode.empty() ? std::string("?") : setup_mode) + " offers " + std::to_string(lists.size()) +
                       " lists); ";
    auto holds_all = [&](const ListTeams &l) {
        if (want.empty())
            return false;
        std::vector<uint16_t> have = distinct_teams(l.teams);
        return std::includes(have.begin(), have.end(), want.begin(), want.end());
    };
    int64_t first = -1, press = -1;
    for (size_t i = 0; i < lists.size(); ++i) {
        if (first < 0 && holds_all(lists[i]))
            first = int64_t(i);
        if (press < 0 && !pressed.empty() && lists[i].name == pressed)
            press = int64_t(i);
    }
    if (press >= 0 && holds_all(lists[size_t(press)]))
        first = press;
    if (first >= 0) {
        p.index = first;
        p.holds = true;
        p.line = head + "loading " + lists[size_t(first)].name + ", which holds all " + std::to_string(want.size()) +
                 " teams of the saved entrants";
        return p;
    }
    if (press >= 0) {
        p.index = press;
        p.line = head + "no list holds the saved entrants' teams (" + team_text(want) + "); loading " + pressed +
                 ", the box pressed last";
        return p;
    }
    p.line = head + "no list holds the saved entrants' teams (" + team_text(want) + ") and no box was pressed";
    return p;
}

struct AliasedPick {
    ContinuePick pick;
    std::vector<uint16_t> teams;
    bool remapped = false;
};

inline AliasedPick continue_list_aliased(const std::vector<ListTeams> &lists, const std::vector<uint16_t> &entrants,
                                         const std::string &pressed, const std::string &setup_mode, const IdAliases &aliases)
{
    AliasedPick out;
    out.pick = continue_list(lists, entrants, pressed, setup_mode);
    out.teams = entrants;
    if (out.pick.holds || !aliases.any())
        return out;
    std::vector<uint16_t> mapped = remap_teams(aliases, entrants);
    if (mapped == entrants)
        return out;
    ContinuePick again = continue_list(lists, mapped, pressed, setup_mode);
    if (!again.holds)
        return out;
    out.pick = again;
    out.pick.line += " after mapping the saved team ids " + team_text(distinct_teams(entrants)) + " to " +
                     team_text(distinct_teams(mapped)) + " (the ids the rows had before this build)";
    out.teams = mapped;
    out.remapped = true;
    return out;
}

inline std::string track_code_of(const std::string &track)
{
    size_t slash = track.rfind('/');
    return slash == std::string::npos ? track : track.substr(slash + 1);
}

inline std::string track_missing(const swofty_core::apply::tracks::Base &base, const std::string &track)
{
    if (!base.ready || track.empty() || base.by_id(track))
        return {};
    std::string code = track_code_of(track);
    if (base.by_code(code))
        return {};
    // A stock track is in every build, also when the base holds no tracks because no track addon is on.
    if (track.starts_with("game/") && swofty_core::apply::tracks::stock_venue(code))
        return {};
    std::string why = "the saved weekend's track " + track + " is not a track of this build";
    if (swofty_core::apply::tracks::is_dormant(code))
        why += " (" + code + " is a dormant row with no track archive; the addon that fills it is off)";
    return why;
}

struct CareerListPick {
    int64_t index = -1;
    std::string line;
};

inline CareerListPick career_list(const std::vector<std::string> &lists, const std::string &data_group,
                                  const std::vector<std::string> &content, const std::string &setup_mode)
{
    CareerListPick p;
    std::string head = "the game left the entrants manager's list empty in a Driver Career (setup mode " +
                       (setup_mode.empty() ? std::string("?") : setup_mode) + " offers " + std::to_string(lists.size()) +
                       " lists); ";
    auto find = [&](const std::string &name) -> int64_t {
        if (name.empty())
            return -1;
        for (size_t i = 0; i < lists.size(); ++i)
            if (lists[i] == name)
                return int64_t(i);
        return -1;
    };
    p.index = find(data_group);
    if (p.index >= 0) {
        p.line = head + "loading " + lists[size_t(p.index)] + ", the list named like the career's data group";
        return p;
    }
    for (const std::string &c : content) {
        p.index = find(c);
        if (p.index >= 0) {
            p.line = head + "loading " + lists[size_t(p.index)] + ", the list of the content the career runs";
            return p;
        }
    }
    p.line = head + "no list is named like the career's data group (" + (data_group.empty() ? std::string("none") : data_group) +
             ") or its content";
    return p;
}

inline std::string career_stop(const CareerListPick &pick)
{
    return "Driver Career load stopped: " + pick.line +
           ". F1 25 closes now instead of crashing on it; start a new Driver Career, or turn the addons back on that the "
           "career was made with";
}

inline std::string continue_stop(const std::string &track_why, const ContinuePick &pick, bool list_missing)
{
    std::vector<std::string> why;
    if (!track_why.empty())
        why.push_back(track_why);
    if (list_missing && pick.index < 0)
        why.push_back(pick.line);
    if (why.empty())
        return {};
    std::string s = "Grand Prix load stopped: ";
    for (size_t i = 0; i < why.size(); ++i)
        s += (i ? "; " : "") + why[i];
    return s + ". F1 25 closes now instead of crashing on it; turn the addon that made the save back on, or start a "
               "new Grand Prix in place of Continue";
}

}
