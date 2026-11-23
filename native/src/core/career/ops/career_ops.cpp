#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#include "core/career/ops/seat_plan.hpp"
#include "core/career/standings_read.hpp"
#include "core/career/standings_snapshot.hpp"
#include "core/runtime/build.hpp"
#include "core/career/ops/stats.hpp"
#include "core/apply/content/teams.hpp"
#include "core/hooks/hooks.hpp"
#include "core/registry/table.hpp"
#include "core/career/ops/track_fuel.hpp"
#include "core/career/ops/entrants.hpp"
#include "core/career/ops/lineup.hpp"
#include "core/career/ops/ops_api.hpp"
#include "core/career/ops/report_queue.hpp"
#include "core/career/ops/tables.hpp"
#include "core/runtime/loader/core_rank.hpp"

// The core's career operations (standard library 1.6, SwoftyCareerOps): apply_lineup, set_driver_ratings and
// swap_drivers, (table version 2) report_season, which the career snapshots answer (snapshots/runtime.cpp), and
// (table version 4) set_seat, which runs one seat change through apply_lineup's executor, set_car_number (Career
// Mode Editor's drivers.number.set) and set_track_fuel (track_fuel.cpp). The line-up executor is Progressive Career's season batch (its native/src/
// season.cpp apply(), seats.cpp) moved into the core: one batch of entrant writes through the Driver Career
// sequence, read back and written back on a mismatch, the seated drivers' contracts, numbers and one
// finalise. Every binding resolves when an operation first needs it; a missing one makes the operation
// report unavailable and log its id.
// The executor is lineup.cpp, the entrant rows, writes and contracts it drives are entrants.cpp, and apply_teams,
// set_car_number and the label lookups are apply_teams.cpp, car_numbers.cpp and labels.cpp.

namespace career::ops::live {

Bindings bindings;
constinit std::atomic<uint32_t> flow_tid{0};
Tables tables;

}

namespace {

using namespace career::ops::live;
namespace g = swofty::game;
namespace mem = swofty::mem;

int api_available() { return runtime::enabled("career", nullptr) ? 1 : 0; }

int api_driver_stat(const char *ref, uint32_t *out)
{
    if (!ref || !out)
        return SWOFTY_ASSET_INVALID;
    if (!labels())
        return SWOFTY_ASSET_LATE;
    int s = resolve_driver(ref);
    if (s < 0)
        return SWOFTY_ASSET_INVALID;
    *out = static_cast<uint32_t>(s);
    return SWOFTY_ASSET_OK;
}

int api_set_slots(const char *group, const uint32_t *teams, size_t count)
{
    if (!group || !*group || (count && !teams) || count > 64)
        return SWOFTY_ASSET_INVALID;
    std::vector<uint32_t> v(teams, teams + count);
    std::string list;
    for (size_t i = 0; i < count; ++i) {
        if (teams[i] >= 0x3fff)
            return SWOFTY_ASSET_INVALID;
        list += (i ? " " : "") + std::to_string(teams[i]);
    }
    {
        std::lock_guard<std::mutex> l(tables.lock);
        tables.slots[group] = std::move(v);
    }
    swofty::log("[career-ops] set_slots %s: %zu slot(s): %s", group, count, list.c_str());
    return SWOFTY_ASSET_OK;
}

int api_apply_lineup(SwoftyText request, char *out, size_t cap, size_t *len)
{
    Report rep;
    std::string summary, head = "apply_lineup";
    int r = SWOFTY_ASSET_OK;
    json::Value j;
    std::string text = request.data ? std::string(request.data, request.size) : std::string();
    if (!on_flow_thread()) {
        rep.result = "failed";
        summary = "late: called off the flow thread";
        r = SWOFTY_ASSET_LATE;
    } else if (!json::parse(text, j) || j.type != json::Value::Obj) {
        rep.result = "refused";
        summary = "the request is not a JSON object";
        r = SWOFTY_ASSET_INVALID;
    } else if (j["lineup"].type == json::Value::Obj) {
        Lineup l;
        std::string err;
        head = "apply_lineup " + std::to_string(number(j["lineup"]["year"], 0)) + " (lineup";
        head += j["lineup"]["why"].str().empty() ? ")" : ": " + j["lineup"]["why"].str() + ")";
        if (!read_lineup(j["lineup"], l, err)) {
            rep.result = "refused";
            summary = err;
            r = SWOFTY_ASSET_INVALID;
        } else {
            r = execute(l, rep, summary);
            rep.changed = rep.written;
        }
    } else if (j["season"].type == json::Value::Obj) {
        career::ops::plan::Input in;
        int year = 0;
        std::string err;
        read_season(j["season"], in, year, err);
        head = "apply_lineup " + std::to_string(year) + " (season, " + (in.data_group.empty() ? "no data group" : in.data_group) +
               ")";
        std::string missing;
        uintptr_t e = 0;
        std::vector<Row> rows;
        if (!lineup_bound(missing)) {
            rep.result = "failed";
            summary = "unavailable: binding(s) not resolved: " + missing;
            r = SWOFTY_ASSET_UNAVAILABLE;
        } else if (!labels()) {
            rep.result = "failed";
            summary = "the drivers table is not loaded yet";
            r = SWOFTY_ASSET_LATE;
        } else if (!(e = entity()) || !entrant_rows(e, rows)) {
            rep.result = "failed";
            summary = "no career entity or its entrant rows are unreadable";
            r = SWOFTY_ASSET_LATE;
        } else {
            career::ops::plan::Plan p = career::ops::plan::plan(in, plan_rows(rows));
            Lineup l;
            l.year = year;
            l.why = "season record";
            l.player_team = p.player_team;
            for (auto &m : p.moves)
                l.moves.push_back({m.driver, m.team, m.seat, -1, -1, -1});
            l.releases = p.releases;
            rep.refusals = p.refusals;
            rep.refused = p.refusals.size();
            Report run;
            r = execute(l, run, summary);
            rep.result = run.result;
            rep.written = run.written;
            rep.released = run.released;
            rep.numbers = run.numbers;
            rep.contracts = run.contracts;
            rep.finalise = run.finalise;
            rep.changes = run.changes;
            rep.unchanged = p.unchanged;
            rep.changed = r == SWOFTY_ASSET_OK ? p.moves.size() : 0;
        }
    } else {
        rep.result = "refused";
        summary = "the request has neither \"season\" nor \"lineup\"";
        r = SWOFTY_ASSET_INVALID;
    }
    rep.refused = std::max(rep.refused, rep.refusals.size());
    swofty::log("[career-ops] %s: %zu seat(s) set, %zu unchanged, %zu refused; %s", head.c_str(), rep.changed, rep.unchanged,
                rep.refused, summary.c_str());
    for (auto &why : rep.refusals)
        swofty::log("[career-ops]   refused: %s", why.c_str());
    if (rep.result == "ok" && r != SWOFTY_ASSET_OK)
        rep.result = "failed";
    int w = put(rep.json(), out, cap, len);
    return w == SWOFTY_TOO_SMALL && r == SWOFTY_ASSET_OK ? SWOFTY_TOO_SMALL : r;
}

int api_set_driver_ratings(uint32_t driver, const uint16_t *ids, const float *values, size_t count)
{
    if (!ids || !values || !count || count > 16 || driver >= 0x3fff)
        return SWOFTY_ASSET_INVALID;
    if (!on_flow_thread()) {
        swofty::log("[career-ops] set_driver_ratings %u: late: called off the flow thread", driver);
        return SWOFTY_ASSET_LATE;
    }
    labels();
    std::string text;
    int r = career::ops::stats::set(driver, ids, values, count, text);
    swofty::log("[career-ops] set_driver_ratings %s: %s", driver_name(driver).c_str(), text.c_str());
    return r;
}

int api_swap_drivers(uint32_t a, uint32_t b)
{
    std::string summary;
    int r = SWOFTY_ASSET_OK;
    Report rep;
    labels();
    std::string head = "swap_drivers " + driver_name(a) + " <-> " + driver_name(b);
    std::string missing;
    uintptr_t e = 0;
    std::vector<Row> rows;
    if (!on_flow_thread()) {
        summary = "late: called off the flow thread";
        r = SWOFTY_ASSET_LATE;
    } else if (a >= 0x3fff || b >= 0x3fff || a == b) {
        summary = "two different driver stat ids are needed";
        r = SWOFTY_ASSET_INVALID;
    } else if (!lineup_bound(missing)) {
        summary = "unavailable: binding(s) not resolved: " + missing;
        r = SWOFTY_ASSET_UNAVAILABLE;
    } else if (!(e = entity()) || !entrant_rows(e, rows)) {
        summary = "no career entity or its entrant rows are unreadable";
        r = SWOFTY_ASSET_LATE;
    } else {
        int ia = row_index(rows, a), ib = row_index(rows, b);
        auto seated = [&](int i) {
            return i >= 0 && has_car(rows[i].rec) && !teamless(rows[i].rec) && position_of(rows[i].rec) >= 0;
        };
        if (ia < 0 || ib < 0) {
            summary = "refused: " + driver_name(ia < 0 ? a : b) + " has no single career row";
            r = SWOFTY_ASSET_INVALID;
        } else if (human(rows[ia].rec) || human(rows[ib].rec)) {
            summary = "refused: the player is never swapped";
            r = SWOFTY_ASSET_INVALID;
        } else if (!seated(ia) || !seated(ib)) {
            summary = "refused: " + driver_name(seated(ia) ? b : a) + " has no race seat";
            r = SWOFTY_ASSET_INVALID;
        } else {
            Lineup l;
            l.why = "swap";
            for (auto &row : rows)
                if (human(row.rec) && !teamless(row.rec))
                    l.player_team = static_cast<int>(stat_of(row.rec.team_stat_id));
            l.moves.push_back({a, static_cast<int>(stat_of(rows[ib].rec.team_stat_id)), position_of(rows[ib].rec), -1,
                               static_cast<int>(b), -1});
            l.moves.push_back({b, static_cast<int>(stat_of(rows[ia].rec.team_stat_id)), position_of(rows[ia].rec), -1,
                               static_cast<int>(a), -1});
            r = execute(l, rep, summary);
        }
    }
    swofty::log("[career-ops] %s: %s", head.c_str(), summary.c_str());
    return r;
}

career::ops::reports::Queue season_reports;

const char *result_name(int r)
{
    return r == SWOFTY_ASSET_OK            ? "ok"
           : r == SWOFTY_ASSET_LATE        ? "late"
           : r == SWOFTY_ASSET_UNAVAILABLE ? "unavailable"
           : r == SWOFTY_REFUSED           ? "refused"
                                           : "invalid";
}

int api_report_season(int32_t year, int32_t season_index)
{
    std::string dll, who = settings_addon(__builtin_return_address(0), dll);
    if (who.empty())
        who = dll.empty() ? std::string("an unknown module") : dll;
    if (!on_flow_thread()) {
        namespace rq = career::ops::reports;
        rq::Queued q{who, runtime::snapshots_career_now(), year, season_index, GetCurrentThreadId()};
        swofty::log("%s", (season_reports.put(q) ? rq::queued_line(q) : rq::full_line(q)).c_str());
        return SWOFTY_ASSET_LATE;
    }
    return runtime::snapshots_report_season(who, year, season_index);
}

void run_queued_reports()
{
    if (!season_reports.pending())
        return;
    namespace rq = career::ops::reports;
    for (auto &r : rq::drain(season_reports, runtime::snapshots_career_now(), runtime::snapshots_report_season))
        swofty::log("%s", rq::ran_line(r, result_name(r.result)).c_str());
}

void refresh_standings();

void flow_frame(double dt, void *user)
{
    frame(dt, user);
    run_queued_reports();
    refresh_standings();
}

std::string caller(const void *from)
{
    std::string dll, who = settings_addon(from, dll);
    return who.empty() ? (dll.empty() ? std::string("an unknown module") : dll) : who;
}

std::string loaded_data_group()
{
    char group[128] = {};
    if (auto t = runtime::career_content_table();
        t && t->data_group && t->data_group(nullptr, group, sizeof group) == SWOFTY_ASSET_OK)
        return group;
    return {};
}

int api_set_seat(SwoftyText request, char *out, size_t cap, size_t *len)
{
    Report rep;
    std::string summary, head = "set_seat";
    int r = SWOFTY_ASSET_OK;
    json::Value j;
    std::string text = request.data ? std::string(request.data, request.size) : std::string();
    career::ops::seat::Plan sp;
    labels();
    if (!on_flow_thread()) {
        rep.result = "failed";
        summary = "late: called off the flow thread";
        r = SWOFTY_ASSET_LATE;
    } else if (!json::parse(text, j) || j.type != json::Value::Obj) {
        rep.result = "refused";
        summary = "the request is not a JSON object";
        r = SWOFTY_ASSET_INVALID;
    } else {
        int driver = number(j["driver"]), team = number(j["team"]), slot = number(j["slot"]), seat = number(j["seat"]);
        std::string group;
        if (team < 0 && slot >= 1) {
            group = loaded_data_group();
            std::lock_guard<std::mutex> l(tables.lock);
            auto it = tables.slots.find(group);
            if (it != tables.slots.end() && static_cast<size_t>(slot) <= it->second.size())
                team = static_cast<int>(it->second[slot - 1]);
        }
        head = "set_seat " + (driver >= 0 ? driver_name(static_cast<uint32_t>(driver)) : std::string("?")) + " -> " +
               (team >= 0 ? seat_text(static_cast<uint32_t>(team), seat)
                          : "slot " + std::to_string(slot) + " seat " + std::to_string(seat + 1));
        std::string missing;
        uintptr_t e = 0;
        std::vector<Row> rows;
        if (driver < 0 || driver >= 0x3fff) {
            rep.result = "refused";
            summary = "\"driver\" must be a driver stat id 0..16382";
            r = SWOFTY_ASSET_INVALID;
        } else if (team < 0) {
            rep.result = "refused";
            summary = slot >= 1 ? "slot " + std::to_string(slot) + " is not in the slot table of data group " +
                                      (group.empty() ? std::string("(none loaded)") : group)
                                : std::string("the request names neither \"team\" nor \"slot\"");
            r = SWOFTY_ASSET_INVALID;
        } else if (!lineup_bound(missing)) {
            rep.result = "failed";
            summary = "unavailable: binding(s) not resolved: " + missing;
            r = SWOFTY_ASSET_UNAVAILABLE;
        } else if (!(e = entity()) || !entrant_rows(e, rows)) {
            rep.result = "failed";
            summary = "no career entity or its entrant rows are unreadable";
            r = SWOFTY_ASSET_LATE;
        } else {
            sp = career::ops::seat::plan(plan_rows(rows), static_cast<uint32_t>(driver), team, seat);
            if (!sp.refusal.empty()) {
                rep.result = "refused";
                rep.refusals.push_back(sp.refusal);
                rep.refused = 1;
                summary = "refused: " + sp.refusal;
                r = SWOFTY_ASSET_INVALID;
            } else if (sp.unchanged) {
                rep.unchanged = 1;
                summary = "already on that seat";
            } else {
                Lineup l;
                l.why = "set_seat";
                l.player_team = sp.player_team;
                l.moves.push_back({sp.driver, sp.team, sp.seat, number(j["car"]), sp.holder, -1});
                if (sp.what == career::ops::seat::Holder::swapped)
                    l.moves.push_back(
                        {static_cast<uint32_t>(sp.holder), sp.from_team, sp.from_seat, -1, static_cast<int>(sp.driver), -1});
                else if (sp.what == career::ops::seat::Holder::released)
                    l.releases.push_back(static_cast<uint32_t>(sp.holder));
                r = execute(l, rep, summary);
                rep.changed = r == SWOFTY_ASSET_OK ? 1 : 0;
            }
        }
    }
    swofty::log("[career-ops] %s: %s", head.c_str(), summary.c_str());
    if (rep.result == "ok" && r != SWOFTY_ASSET_OK)
        rep.result = "failed";
    std::string body = rep.json();
    body.insert(body.size() - 1, ",\"holder\":" + quote(career::ops::seat::name(sp.what)));
    int w = put(body, out, cap, len);
    return w == SWOFTY_TOO_SMALL && r == SWOFTY_ASSET_OK ? SWOFTY_TOO_SMALL : r;
}

int api_set_track_fuel(SwoftyText request, char *out, size_t cap, size_t *len)
{
    std::string who = caller(__builtin_return_address(0));
    if (!on_flow_thread()) {
        swofty::log("[career-ops] set_track_fuel from %s: late: called off the flow thread", who.c_str());
        return SWOFTY_ASSET_LATE;
    }
    return career::ops::set_track_fuel(who, request, out, cap, len);
}

int api_set_logo(const char *team_id, const char *logo_id)
{
    if (!team_id || !logo_id || !*team_id || !*logo_id)
        return SWOFTY_ASSET_INVALID;
    namespace reg = swofty_core::registry;
    reg::Store &st = reg::store();
    bool stock = std::string_view(team_id).rfind("game/", 0) == 0;
    if ((!stock && st.get("team.identity", team_id).status != reg::Status::ok) ||
        st.get("team.logo", logo_id).status != reg::Status::ok) {
        swofty::log("[career-ops] set_logo %s -> %s: refused: no such %s record", team_id, logo_id,
                    !stock && st.get("team.identity", team_id).status != reg::Status::ok ? "team.identity" : "team.logo");
        return SWOFTY_REFUSED;
    }
    {
        std::lock_guard<std::recursive_mutex> g(swofty_core::apply::content::live_mutex());
        swofty_core::apply::content::live().logo_override[team_id] = logo_id;
    }
    swofty_core::hooks::keep_logo_point();
    swofty::log("[career-ops] set_logo %s -> %s: the team's logo for this session", team_id, logo_id);
    return SWOFTY_ASSET_OK;
}

int api_set_look(const char *driver_ref, const char *look_ref)
{
    if (!driver_ref || !look_ref || !*driver_ref || !*look_ref)
        return SWOFTY_ASSET_INVALID;
    if (!on_flow_thread()) {
        swofty::log("[career-ops] set_look %s %s: late: called off the flow thread", driver_ref, look_ref);
        return SWOFTY_ASSET_LATE;
    }
    if (!labels()) {
        swofty::log("[career-ops] set_look %s %s: the drivers table is not loaded yet", driver_ref, look_ref);
        return SWOFTY_ASSET_LATE;
    }
    int driver = resolve_driver(driver_ref), look = resolve_look(look_ref);
    if (driver < 0 || look <= 0) {
        swofty::log("[career-ops] set_look %s %s: refused: %s is not a driver or look the game loaded", driver_ref, look_ref,
                    driver < 0 ? driver_ref : look_ref);
        return SWOFTY_ASSET_INVALID;
    }
    std::string request = "{\"lineup\":{\"visuals\":true,\"why\":\"set_look\",\"looks\":[{\"driver\":" + std::to_string(driver) +
                          ",\"look\":" + std::to_string(look) + "}]}}";
    char report[512];
    size_t len = 0;
    int r = api_apply_lineup({request.data(), request.size()}, report, sizeof report, &len);
    swofty::log("[career-ops] set_look %s -> look %s: %s", driver_name(static_cast<uint32_t>(driver)).c_str(), look_ref,
                r == SWOFTY_ASSET_OK ? "applied" : "not applied");
    return r == SWOFTY_TOO_SMALL ? SWOFTY_ASSET_OK : r;
}

int api_team_stat(const char *ref, uint32_t *out)
{
    if (!ref || !out)
        return SWOFTY_ASSET_INVALID;
    if (!labels())
        return SWOFTY_ASSET_LATE;
    int s = resolve_team(ref);
    if (s < 0)
        return SWOFTY_ASSET_INVALID;
    *out = static_cast<uint32_t>(s);
    return SWOFTY_ASSET_OK;
}

// The loaded career's current season: the Season at the Championship's season_index. LATE off the flow thread, with
// no career entity or while it is busy, and while the championship or the season entry doesn't check out;
// UNAVAILABLE when the championship's or the season's vtable doesn't bind.
int current_season(uintptr_t &season, uint32_t &season_index)
{
    if (!on_flow_thread())
        return SWOFTY_ASSET_LATE;
    uintptr_t e = entity();
    uintptr_t champ_vt = bind("vt_Career_Championship"), season_vt = bind("vt_Career_Season");
    if (!champ_vt || !season_vt)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!e)
        return SWOFTY_ASSET_LATE;
    g::CareerEntity ent{};
    if (!mem::read(e, ent) || ent.entrant_update_busy)
        return SWOFTY_ASSET_LATE;
    uintptr_t ch = rdv<uintptr_t>(e + offsetof(g::CareerEntity, current_championship));
    g::Championship champ{};
    if (!ch || rdv<uintptr_t>(ch) != champ_vt || !mem::read(ch, champ))
        return SWOFTY_ASSET_LATE;
    auto b = reinterpret_cast<uintptr_t>(champ.seasons_begin), en = reinterpret_cast<uintptr_t>(champ.seasons_end);
    size_t n = en > b && !((en - b) % sizeof(g::ChampionshipSeasonEntry)) ? (en - b) / sizeof(g::ChampionshipSeasonEntry) : 0;
    if (!n || n > 64 || champ.season_index >= n)
        return SWOFTY_ASSET_LATE;
    g::ChampionshipSeasonEntry entry{};
    if (!mem::read(b + champ.season_index * sizeof entry, entry) || !entry.season ||
        rdv<uintptr_t>(reinterpret_cast<uintptr_t>(entry.season)) != season_vt)
        return SWOFTY_ASSET_LATE;
    season = reinterpret_cast<uintptr_t>(entry.season);
    season_index = static_cast<uint32_t>(champ.season_index);
    return SWOFTY_ASSET_OK;
}

int api_current_round(int32_t *season_index, int32_t *round, int32_t *rounds)
{
    if (!season_index || !round || !rounds)
        return SWOFTY_ASSET_INVALID;
    if (!on_flow_thread())
        return SWOFTY_ASSET_LATE;
    uintptr_t weekend_vt = bind("vt_Career_Weekend");
    if (!weekend_vt)
        return SWOFTY_ASSET_UNAVAILABLE;
    uintptr_t at = 0;
    uint32_t index = 0;
    if (int r = current_season(at, index); r != SWOFTY_ASSET_OK)
        return r;
    g::Season season{};
    if (!mem::read(at, season))
        return SWOFTY_ASSET_LATE;
    auto wb = reinterpret_cast<uintptr_t>(season.weekends_begin), we = reinterpret_cast<uintptr_t>(season.weekends_end);
    size_t weekends = wb && we > wb && !((we - wb) % sizeof(void *)) ? (we - wb) / sizeof(void *) : 0;
    if (!weekends || weekends > 64 || season.current_weekend >= weekends)
        return SWOFTY_ASSET_LATE;
    for (size_t i = 0; i < weekends; ++i) {
        uintptr_t w = rdv<uintptr_t>(wb + i * sizeof(void *));
        if (!w || rdv<uintptr_t>(w) != weekend_vt)
            return SWOFTY_ASSET_LATE;
    }
    *season_index = static_cast<int32_t>(index);
    *round = static_cast<int32_t>(season.current_weekend) + 1;
    *rounds = static_cast<int32_t>(weekends);
    return SWOFTY_ASSET_OK;
}

namespace bp = runtime::build_proof;
namespace cs = career::standings;

// The standings read (standings_read.hpp) walks the season standings' table (+0x40), points (+0x20) and results
// (+0xd0). STANDINGS_LAYOUT (core/guards/career/standings_drop.cpp) proves the table, the points and the season walk on
// both 1.0.145 builds. The results +0xd0/+0xd8 and their 0x10-byte records (the position +4, the status +0xc) are used
// by the same instructions in the points recompute (KB 0x13cdff0) and the results writer (KB 0x13cd970), each equal
// over the whole function on Steam 1.0.145 and EA 1.0.145 (re/tools/sigport.py --witness; re/db/verify.tsv V110002
// to V110005).
constexpr bp::Proof STANDINGS_READ{"the season standings' table, points and results (+0x40, +0x20, +0xd0, results of "
                                   "0x10 bytes)",
                                   bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate STANDINGS{"the standings read", &STANDINGS_READ};

static_assert(offsetof(g::Season, standings_vtable) == cs::sd::SEASON_STANDINGS);
static_assert(offsetof(g::F1SeasonStandings, points_begin) == cs::sd::POINTS_ROUNDS);
static_assert(offsetof(g::F1SeasonStandings, results_begin) == cs::RESULTS_ROUNDS);
static_assert(offsetof(g::F1SeasonStandings, results_end) == cs::RESULTS_ROUNDS + 8);
static_assert(sizeof(g::StandingsRound) == cs::sd::ROUND_SIZE && offsetof(g::StandingsRound, sessions_begin) == 8);
static_assert(sizeof(g::StandingsSession) == cs::sd::SESSION_SIZE && offsetof(g::StandingsSession, records_begin) == 8 &&
              offsetof(g::StandingsSession, session_id) == cs::sd::SESSION_ID);
static_assert(sizeof(g::StandingsPoints) == cs::sd::RECORD_SIZE);
static_assert(offsetof(g::F1SeasonStandings, table_begin) == cs::sd::TABLE_ROUNDS);
static_assert(offsetof(g::F1SeasonStandings, table_end) == cs::sd::TABLE_ROUNDS + 8);
static_assert(sizeof(g::StandingsTableEntry) == cs::sd::RECORD_SIZE && sizeof(cs::sd::Record) == cs::sd::RECORD_SIZE &&
              offsetof(g::StandingsTableEntry, key) == offsetof(cs::sd::Record, key) &&
              offsetof(g::StandingsTableEntry, total) == offsetof(cs::sd::Record, points));
static_assert(sizeof(g::StandingsResult) == cs::RESULT_SIZE && offsetof(g::StandingsResult, position) == cs::RESULT_POSITION &&
              offsetof(g::StandingsResult, status) == cs::RESULT_STATUS);

bool read_game(uint64_t at, void *out, size_t n, void *)
{
    if (!mem::readable(static_cast<uintptr_t>(at), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(at), n);
    return true;
}

int api_standings(char *out, size_t cap, size_t *len)
{
    if (!runtime::holds(STANDINGS))
        return SWOFTY_ASSET_UNAVAILABLE;
    uintptr_t season = 0;
    uint32_t index = 0;
    if (int r = current_season(season, index); r != SWOFTY_ASSET_OK)
        return r;
    cs::Standings s;
    if (!cs::read(season + cs::sd::SEASON_STANDINGS, read_game, nullptr, s))
        return SWOFTY_ASSET_LATE;
    return put(cs::json(s, index), out, cap, len);
}

int api_set_dropped_scores(const char *data_group, uint32_t best)
{
    if (!data_group || !*data_group || best > 64)
        return SWOFTY_ASSET_INVALID;
    return runtime::standings_drop_set(data_group, best);
}

// The championship standings snapshot (SwoftyCoreStandings). The constructors' side and the weekends' line-up copies
// are read by instructions equal on each build (re/tools/sigport.py --witness at the KB RVA): the season loop 0xae9550
// (+0x3b8, 59 of 59), the season-start order builder 0xae2d80 (the constructors' table +0x3f8, +0x400; 341 of 341),
// the constructors' recompute body 0x95e2c60 (+0x20, +0x28, +0x40, +0x48 aligned, 128 of 131), the start-order fill
// 0x13cf8a0 (+0xd0, +0xd8, +0xe0 aligned) and Career_Weekend_SetEntrantNumber 0xaf6800 (the weekend's rows +0x20,
// +0x28; 47 of 47), on Steam 1.0.145 (26566308) and on the EA 1.0.145 dump.
constexpr bp::Proof CONSTRUCTORS_READ{"the constructors' standings (Season +0x3b8: points +0x20, table +0x40, start order "
                                      "+0xd8) and the weekends' line-up copies (+0x20)",
                                      bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) |
                                          bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate CONSTRUCTORS{"the championship standings", &CONSTRUCTORS_READ};
constexpr uint64_t STANDINGS_EVERY_MS = 1000;
constexpr uintptr_t WEEKEND_ROWS = 0x20, SESSION_ID_AT = 0x8;

struct StandingsStore {
    std::mutex lock;
    cs::Snapshot snap;
    int status = SWOFTY_ASSET_LATE;
    uint64_t seq = 0;
    uint64_t next_ms = 0;
    int32_t logged_raced = -1, logged_season = -1;
};

StandingsStore &standings_store()
{
    static StandingsStore s;
    return s;
}

uint32_t stat_or_none(uint32_t key) { return none(key) ? cs::NONE : stat_of(key); }

std::vector<cs::Seat> seats_at(uintptr_t begin_field)
{
    std::vector<cs::Seat> out;
    uintptr_t b = rdv<uintptr_t>(begin_field), e = rdv<uintptr_t>(begin_field + 8);
    if (!b || e <= b || (e - b) % sizeof(EntrantRecord) || (e - b) / sizeof(EntrantRecord) > kMaxEntrants ||
        !mem::readable(b, e - b))
        return out;
    for (uintptr_t p = b; p < e; p += sizeof(EntrantRecord)) {
        EntrantRecord r{};
        if (mem::read(p, r) && has_car(r))
            out.push_back({stat_of(driver_of(r)), teamless(r) ? cs::NONE : stat_of(r.team_stat_id),
                           r.car_number == kNoNumber ? uint8_t(0) : r.car_number});
    }
    return out;
}

std::vector<uint32_t> weekend_sessions(uintptr_t weekend)
{
    std::vector<uint32_t> ids;
    g::Weekend wk{};
    if (!mem::read(weekend, wk))
        return ids;
    auto b = reinterpret_cast<uintptr_t>(wk.sessions_begin), e = reinterpret_cast<uintptr_t>(wk.sessions_end);
    if (!b || e <= b || (e - b) % sizeof(g::Session) || (e - b) / sizeof(g::Session) > 16)
        return ids;
    for (uintptr_t s = b; s < e; s += sizeof(g::Session)) {
        uint32_t id = 0;
        if (mem::read(s + SESSION_ID_AT, id))
            ids.push_back(id);
    }
    return ids;
}

cs::Lineups read_lineups(uintptr_t e, uintptr_t season)
{
    cs::Lineups l;
    std::vector<Row> rows;
    if (entrant_rows(e, rows))
        for (const Row &r : rows)
            if (has_car(r.rec))
                l.now.push_back({stat_of(driver_of(r.rec)), teamless(r.rec) ? cs::NONE : stat_of(r.rec.team_stat_id),
                                 r.rec.car_number == kNoNumber ? uint8_t(0) : r.rec.car_number});
    g::Season s{};
    if (!mem::read(season, s))
        return l;
    auto wb = reinterpret_cast<uintptr_t>(s.weekends_begin), we = reinterpret_cast<uintptr_t>(s.weekends_end);
    if (!wb || we <= wb || (we - wb) % sizeof(void *) || (we - wb) / sizeof(void *) > 64)
        return l;
    for (uintptr_t at = wb; at < we; at += sizeof(void *)) {
        uintptr_t w = rdv<uintptr_t>(at);
        l.rounds.push_back(w ? seats_at(w + WEEKEND_ROWS) : std::vector<cs::Seat>{});
        l.races.push_back(w ? cs::main_race(weekend_sessions(w)) : 0);
    }
    return l;
}

cs::Where read_where(const SwoftyCareerState &state, uint32_t index)
{
    cs::Where w;
    w.mode = state.mode;
    w.season_index = static_cast<int32_t>(index);
    int32_t si = 0;
    if (api_current_round(&si, &w.round, &w.rounds) != SWOFTY_ASSET_OK)
        w.round = w.rounds = 0;
    if (auto facts = swofty_core::hooks::core().current(); facts && facts->career)
        w.year = facts->career->year;
    if (state.has_player) {
        w.player_driver = stat_or_none(state.player_driver);
        w.player_team = stat_or_none(state.player_team);
    }
    return w;
}

std::string rows_text(const std::vector<SwoftyStandingsRow> &rows, bool drivers)
{
    std::string out;
    for (const SwoftyStandingsRow &r : rows) {
        char one[96];
        if (drivers)
            std::snprintf(one, sizeof one, "%sP%d d%u t%d #%u %d (%dw %dp)", out.empty() ? "" : ", ", r.position, r.driver,
                          r.team == cs::NONE ? -1 : int(r.team), r.race_number, r.points, r.wins, r.podiums);
        else
            std::snprintf(one, sizeof one, "%sP%d t%u %d (%dw %dp)", out.empty() ? "" : ", ", r.position, r.team, r.points,
                          r.wins, r.podiums);
        out += one;
    }
    return out.empty() ? "none" : out;
}

void log_standings(StandingsStore &st, const cs::Lineups &l)
{
    const SwoftyStandingsSeason &s = st.snap.season;
    if (s.raced == st.logged_raced && s.season_index == st.logged_season)
        return;
    st.logged_raced = s.raced;
    st.logged_season = s.season_index;
    std::string races;
    for (uint32_t id : l.races)
        races += (races.empty() ? "" : ",") + std::to_string(id);
    swofty::log("[standings] season %d (year %d) round %d of %d: %d round(s) scored, %d raced%s; main races %s", s.season_index,
                s.year, s.round, s.rounds, s.scored, s.raced, s.flags & SWOFTY_STANDINGS_COMPLETE ? ", complete" : "",
                races.empty() ? "unread" : races.c_str());
    swofty::log("[standings] drivers: %s", rows_text(st.snap.drivers, true).c_str());
    swofty::log("[standings] constructors: %s", rows_text(st.snap.constructors, false).c_str());
}

void clear_standings(int status)
{
    StandingsStore &st = standings_store();
    std::lock_guard<std::mutex> g(st.lock);
    st.status = status;
    st.snap = {};
    st.logged_raced = st.logged_season = -1;
}

// A read that fails while the career stays loaded (it is busy, or between seasons) keeps the last snapshot.
void standings_late()
{
    StandingsStore &st = standings_store();
    std::lock_guard<std::mutex> g(st.lock);
    if (st.status != SWOFTY_ASSET_OK)
        st.status = SWOFTY_ASSET_LATE;
}

void publish_standings(const cs::Snapshot &snap, const cs::Lineups &l)
{
    StandingsStore &st = standings_store();
    std::lock_guard<std::mutex> g(st.lock);
    st.status = SWOFTY_ASSET_OK;
    if (cs::same(st.snap, snap))
        return;
    st.snap = snap;
    st.snap.season.seq = ++st.seq;
    log_standings(st, l);
}

void refresh_standings()
{
    StandingsStore &st = standings_store();
    uint64_t now = GetTickCount64();
    if (now < st.next_ms)
        return;
    st.next_ms = now + STANDINGS_EVERY_MS;
    const SwoftyCareer *c = runtime::career_table();
    SwoftyCareerState state{};
    if (!runtime::enabled("career", nullptr) || !c || !c->state || c->state(&state) != SWOFTY_ASSET_OK || !state.loaded ||
        (state.mode != SWOFTY_CAREER_DRIVER && state.mode != SWOFTY_CAREER_MY_TEAM))
        return clear_standings(SWOFTY_ASSET_LATE);
    if (!runtime::holds(STANDINGS) || !runtime::holds(CONSTRUCTORS))
        return clear_standings(SWOFTY_ASSET_UNAVAILABLE);
    uintptr_t season = 0;
    uint32_t index = 0;
    cs::Standings drivers;
    cs::Constructors teams;
    if (current_season(season, index) != SWOFTY_ASSET_OK ||
        !cs::read(season + cs::sd::SEASON_STANDINGS, read_game, nullptr, drivers) ||
        !cs::read_constructors(season + cs::SEASON_CONSTRUCTORS, read_game, nullptr, teams))
        return standings_late();
    cs::Lineups l = read_lineups(entity(), season);
    publish_standings(cs::snapshot(drivers, teams, l, read_where(state, index)), l);
}

int st_available() { return runtime::proven(STANDINGS) && runtime::proven(CONSTRUCTORS) ? 1 : 0; }

int st_season(SwoftyStandingsSeason *out)
{
    if (!out)
        return SWOFTY_ASSET_INVALID;
    StandingsStore &st = standings_store();
    std::lock_guard<std::mutex> g(st.lock);
    if (st.status != SWOFTY_ASSET_OK)
        return st.status;
    *out = st.snap.season;
    return SWOFTY_ASSET_OK;
}

int st_rows(bool drivers, SwoftyStandingsRow *out, uint32_t cap, uint32_t *count)
{
    StandingsStore &st = standings_store();
    std::lock_guard<std::mutex> g(st.lock);
    if (st.status != SWOFTY_ASSET_OK)
        return st.status;
    return cs::copy_rows(drivers ? st.snap.drivers : st.snap.constructors, out, cap, count);
}

int st_drivers(SwoftyStandingsRow *out, uint32_t cap, uint32_t *count) { return st_rows(true, out, cap, count); }

int st_constructors(SwoftyStandingsRow *out, uint32_t cap, uint32_t *count) { return st_rows(false, out, cap, count); }

const SwoftyStandings g_standings = {SWOFTY_STANDINGS_VERSION, st_available, st_season, st_drivers, st_constructors};

const SwoftyCareerOps g_table = {
    SWOFTY_CAREER_OPS_VERSION,
    api_available,
    api_driver_stat,
    api_set_slots,
    api_apply_lineup,
    api_set_driver_ratings,
    api_swap_drivers,
    api_report_season,
    api_apply_teams,
    api_set_seat,
    api_set_car_number,
    api_set_track_fuel,
    api_set_logo,
    api_set_look,
    api_current_round,
    api_team_stat,
    api_set_dropped_scores,
    api_standings,
};

void setup()
{
    const SwoftyEvents *ev = runtime::events_table();
    uint32_t token = 0;
    if (ev && ev->on_frame) {
        runtime::frame_probe_name("career_ops");
        ev->on_frame(flow_frame, nullptr, &token);
    }
    career::ops::track_fuel_setup();
}

}

extern "C" __declspec(dllexport) const SwoftyCareerOps *SwoftyCoreCareerOps(void) { return &g_table; }

extern "C" __declspec(dllexport) const SwoftyStandings *SwoftyCoreStandings(void) { return &g_standings; }

SWOFTY_CORE_RANK(career_ops)

SWOFTY_ON_LOAD { runtime::when_code_ready("career operations: flow thread", setup); }
