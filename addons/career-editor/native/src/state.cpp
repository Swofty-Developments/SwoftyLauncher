// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: career-loaded, mode and safe-state tracking, on the core's career library
// (swofty/career.hpp, swofty.core >= 0.2.11).
//
// The core installs the flow-state site (site_FeFlow_SetStateStatus) and the Team HQ answer site
// (site_Career_TeamHqIsMyTeamResult), polls the championship entity's components, and publishes
// flow.state, career.loaded, career.unloaded and career.hub. This file reads the core's getters:
// swofty::career::state() for career, mode, player and how the mode was decided, and
// swofty::career::flow_status(hash) for the status and age of each flow state below. plugin.cpp
// logs the core's career events.
//
// Safe: a career is loaded, StateTeamHQUpdate has been Updating for kSettleMs, and none of the
// Team HQ work states (time advance, to session, end of season, season break) nor a session is
// active.

#include <array>
#include <cstdint>
#include <cstdio>
#include <mutex>
#include <string>

#include <swofty/career.hpp>
#include <swofty/systems/career.hpp>
#include <swofty/mixin.hpp>

#include "bridge.hpp"

namespace {

namespace career = swofty::career;

enum Slot : int {
    TeamHq,
    TeamHqUpdate,
    TeamHqTimeAdvance,
    TeamHqToSession,
    TeamHqEndOfSeason,
    TeamHqSeasonBreak,
    SessionState,
    LoadSession,
    UnloadSession,
    CareerHubUpdate,
    PostRaceCareerHub,
    CareerHubSelector,
    Frontend,
    OnlineFrontend,
    FrontendWorld,
    CareerDataSync,
    SlotCount
};

struct Known {
    const char *name;
    uint32_t hash;
};

constexpr std::array<Known, SlotCount> kKnown = {{
    {"StateTeamHQ", career::state_hash("StateTeamHQ")},
    {"StateTeamHQUpdate", career::state_hash("StateTeamHQUpdate")},
    {"StateTeamHQTimeAdvance", career::state_hash("StateTeamHQTimeAdvance")},
    {"StateTeamHQToSession", career::state_hash("StateTeamHQToSession")},
    {"StateTeamHQEndOfSeason", career::state_hash("StateTeamHQEndOfSeason")},
    {"StateTeamHQSeasonBreak", career::state_hash("StateTeamHQSeasonBreak")},
    {"StateSession", career::state_hash("StateSession")},
    {"StateLoadSession", career::state_hash("StateLoadSession")},
    {"StateUnloadSession", career::state_hash("StateUnloadSession")},
    {"SessionStateCareerHubUpdate", career::state_hash("SessionStateCareerHubUpdate")},
    {"PostRaceCareerHub", career::state_hash("PostRaceCareerHub")},
    {"StateCareerHubSelector", career::state_hash("StateCareerHubSelector")},
    {"StateFrontend", career::state_hash("StateFrontend")},
    {"StateOnlineFrontend", career::state_hash("StateOnlineFrontend")},
    {"StateFrontendWorld", career::state_hash("StateFrontendWorld")},
    {"StateCareerDataSync", career::state_hash("StateCareerDataSync")},
}};

static_assert(career::state_hash("StateTeamHQUpdate") == 0x0e245469u, "state name hashing drifted");
static_assert(career::state_hash("StateTeamHQToSession") == 0xc5dd1dcdu, "state name hashing drifted");

constexpr uint64_t kSettleMs = 750;

SwoftyCareerState core_state()
{
    SwoftyCareerState s{};
    if (career::state(&s) != career::Result::ok)
        s = SwoftyCareerState{};
    return s;
}

int status(Slot s)
{
    career::Status v = career::Status::none;
    career::flow_status(kKnown[s].hash, &v);
    return static_cast<int>(v);
}

uint64_t age(Slot s)
{
    career::Status v = career::Status::none;
    uint64_t a = 0;
    career::flow_status(kKnown[s].hash, &v, &a);
    return a;
}

bool any_session()
{
    return status(SessionState) || status(LoadSession) || status(UnloadSession);
}

bool teamhq_busy()
{
    return status(TeamHqTimeAdvance) || status(TeamHqToSession) || status(TeamHqEndOfSeason) ||
           status(TeamHqSeasonBreak);
}

ce::Mode to_mode(uint32_t m)
{
    return m == SWOFTY_CAREER_MY_TEAM  ? ce::Mode::MyTeam
           : m == SWOFTY_CAREER_DRIVER ? ce::Mode::DriverCareer
                                       : ce::Mode::None;
}

const char *components_name(uint32_t c)
{
    return c == SWOFTY_CAREER_COMPONENTS_LIVE       ? "live"
           : c == SWOFTY_CAREER_COMPONENTS_FALLBACK ? "fallback to flow"
                                                    : "no entity seen yet";
}

struct Logged {
    bool career = false;
    ce::Mode mode = ce::Mode::None;
    bool unavailable = false;
};

Logged g_logged;

} // namespace

namespace ce {

Mode mode()
{
    SwoftyCareerState s = core_state();
    return s.loaded ? to_mode(s.mode) : Mode::None;
}

bool in_career()
{
    return core_state().loaded != 0;
}

bool safe()
{
    return state::blocked(false).empty();
}

namespace {
std::mutex g_instance_mu;
Instance g_instance;
}

std::string career_id()
{
    SwoftyCareerState s = core_state();
    std::string saves = s.loaded ? career::saves::current() : std::string();
    std::lock_guard<std::mutex> lock(g_instance_mu);
    return g_instance.resolve(s.loaded != 0, s.key, saves);
}

std::string career_renamed_to(const std::string &from)
{
    std::lock_guard<std::mutex> lock(g_instance_mu);
    return g_instance.renamed_to(from);
}

std::string legacy_career_key()
{
    return core_state().key;
}

namespace state {

const char *mode_name(Mode m)
{
    return m == Mode::MyTeam ? "my_team" : m == Mode::DriverCareer ? "driver_career" : "none";
}

void poll(uint64_t)
{
    if (!career::available()) {
        if (!g_logged.unavailable)
            log("the core's career library is unavailable (swofty.core older than 0.2.11, the career feature off, "
                "or its flow-state site missing on this build): no career is reported");
        g_logged.unavailable = true;
        return;
    }
    if (g_logged.unavailable)
        log("the core's career library is available again");
    g_logged.unavailable = false;
    SwoftyCareerState s = core_state();
    bool c = s.loaded != 0;
    Mode m = c ? to_mode(s.mode) : Mode::None;
    if (c != g_logged.career || m != g_logged.mode) {
        log("career %s, mode %s (components %s, component mode %s; Team HQ answer %s)", c ? "loaded" : "not loaded",
            mode_name(m), components_name(s.components), mode_name(to_mode(s.component_mode)),
            mode_name(to_mode(s.team_hq_mode)));
        g_logged.career = c;
        g_logged.mode = m;
    }
}

const char *phase()
{
    if (status(TeamHqUpdate) == 2 && !teamhq_busy())
        return "team_hq";
    if (status(TeamHq) || status(TeamHqUpdate) || teamhq_busy())
        return "team_hq_busy";
    if (status(CareerHubUpdate) == 2)
        return "weekend_hub";
    if (status(SessionState) == 2)
        return "session";
    if (status(LoadSession) || status(UnloadSession) || status(SessionState) || status(CareerDataSync))
        return "loading";
    if (status(Frontend) || status(OnlineFrontend) || status(FrontendWorld))
        return "frontend";
    return "unknown";
}

std::string blocked(bool anytime)
{
    SwoftyCareerState s = core_state();
    if (!s.loaded)
        return career::available() ? "no career is loaded" : "the core's career library is unavailable";
    if (s.humans > 1)
        return "two-player careers are not supported";
    if (anytime) {
        if (status(LoadSession) || status(UnloadSession) || status(CareerDataSync))
            return "the game is loading";
        return {};
    }
    if (status(TeamHqTimeAdvance))
        return "Team HQ is advancing time";
    if (status(TeamHqToSession))
        return "Team HQ is heading to a session";
    if (status(TeamHqEndOfSeason))
        return "Team HQ is running the end of season";
    if (status(TeamHqSeasonBreak))
        return "Team HQ is in the season break";
    if (any_session())
        return "a session is running; edits wait for Team HQ";
    if (status(TeamHqUpdate) != 2)
        return "not in Team HQ";
    if (age(TeamHqUpdate) < kSettleMs)
        return "Team HQ is settling";
    return {};
}

bool sections_ok()
{
    SwoftyCareerState s = core_state();
    if (!s.loaded)
        return false;
    if (status(LoadSession) || status(UnloadSession) || status(CareerDataSync))
        return false;
    return (status(TeamHqUpdate) == 2 && !teamhq_busy()) || status(CareerHubUpdate) == 2 ||
           (status(SessionState) == 2 && s.components == SWOFTY_CAREER_COMPONENTS_LIVE && s.component_mode);
}

void write_status(Writer &w, uint64_t)
{
    SwoftyCareerState s = core_state();
    bool career_loaded = s.loaded != 0;
    Mode m = career_loaded ? to_mode(s.mode) : Mode::None;
    std::string why = blocked(false);
    std::string why_any = blocked(true);

    w.begin_object();
    w.key("game");
    w.str("running");
    w.key("career");
    w.boolean(career_loaded);
    w.key("mode");
    w.str(mode_name(m));
    w.key("career_id");
    if (!s.key[0])
        w.null();
    else
        w.str(s.key);
    w.key("humans");
    w.integer(s.humans);
    w.key("phase");
    w.str(phase());
    w.key("safe");
    w.boolean(why.empty());
    w.key("anytime");
    w.boolean(why_any.empty());
    w.key("why");
    if (why.empty())
        w.null();
    else
        w.str(why);
    w.key("why_anytime");
    if (why_any.empty())
        w.null();
    else
        w.str(why_any);

    const Json *season = bridge::section("season");
    auto copy_num = [&](const char *out, const char *in) {
        w.key(out);
        if (season && season->type == Json::Obj && (*season)[in].type == Json::Num)
            w.num((*season)[in].n);
        else
            w.null();
    };
    copy_num("season", "season_number");
    copy_num("year", "year");
    copy_num("round", "round");
    copy_num("rounds", "rounds");
    copy_num("day", "day");
    w.key("weekend");
    if (season && season->type == Json::Obj && (*season)["weekend"].type == Json::Str)
        w.str((*season)["weekend"].s);
    else
        w.null();

    w.key("player");
    if (career_loaded && s.has_player) {
        w.begin_object();
        w.key("driver");
        w.integer(s.player_driver);
        w.key("team");
        w.integer(s.player_team);
        w.end_object();
    } else {
        w.null();
    }

    w.key("detect");
    w.begin_object();
    w.key("components");
    w.str(career::available() ? components_name(s.components) : "core career library unavailable");
    w.key("component_mode");
    w.str(mode_name(to_mode(s.component_mode)));
    w.key("team_hq_answer");
    w.str(mode_name(to_mode(s.team_hq_mode)));
    w.key("flow_career");
    w.boolean(s.flow_career != 0);
    w.end_object();

    career::Flow last{};
    bool have_last = career::flow(&last) == career::Result::ok;
    w.key("state");
    w.str(have_last ? last.name : "");
    w.key("transitions");
    w.integer(have_last ? int64_t(last.transitions) : 0);

    w.key("flow");
    w.begin_array();
    uint64_t wall = wall_ms();
    static career::Status seen_status[SlotCount];
    static uint64_t seen_since[SlotCount];
    for (int i = 0; i < SlotCount; ++i) {
        career::Status st = career::Status::none;
        uint64_t a = 0;
        if (career::flow_status(kKnown[i].hash, &st, &a) != career::Result::ok || st == career::Status::none)
            continue;
        uint64_t since = wall >= a ? wall - a : 0;
        uint64_t drift = since > seen_since[i] ? since - seen_since[i] : seen_since[i] - since;
        if (st != seen_status[i] || drift > 2000)
            seen_status[i] = st, seen_since[i] = since;
        w.begin_object();
        w.key("state");
        w.str(kKnown[i].name);
        w.key("status");
        w.str(career::name(st));
        w.key("since");
        w.integer(int64_t(seen_since[i]));
        w.end_object();
    }
    w.end_array();
    w.end_object();
}

} // namespace state

} // namespace ce
