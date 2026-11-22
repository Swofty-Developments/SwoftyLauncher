#include "core/career/ops/ops_api.hpp"

#include "core/runtime/runtime.hpp"

#include <algorithm>
#include <map>
#include <mutex>
#include <string>
#include <vector>

#include "core/career/ops/start_teams.hpp"
#include "core/career/ops/teams_store.hpp"
#include "core/registry/table.hpp"
#include "core/career/ops/lineup.hpp"
#include "core/career/ops/tables.hpp"

namespace career::ops::live {

std::vector<std::string> slot_rows_of(const std::vector<uint32_t> &slot_teams)
{
    std::vector<std::string> rows;
    std::lock_guard<std::mutex> l(tables.lock);
    for (uint32_t stat : slot_teams) {
        auto name = tables.labels.team_by_stat.find(stat);
        rows.push_back(name == tables.labels.team_by_stat.end() ? std::string() : name->second);
    }
    return rows;
}

StartState take_start(const std::string &group)
{
    std::lock_guard<std::mutex> l(tables.lock);
    auto it = tables.starts.find(group);
    if (it == tables.starts.end())
        return {};
    StartState s = std::move(it->second);
    tables.starts.erase(it);
    return s;
}

int start_teams(const json::Value &s, Report &rep, std::string &head, std::string &summary)
{
    namespace ct = career::ops::teams;
    std::string group = s["data_group"].str();
    int year = number(s["year"], 0);
    head = "apply_teams " + std::to_string(year) + " (start, " + (group.empty() ? "no data group" : group) + ")";
    if (group.empty()) {
        rep.result = "refused";
        summary = "the request names no data group";
        return SWOFTY_ASSET_INVALID;
    }
    std::vector<uint32_t> slot_teams;
    bool have_slots = false;
    {
        std::lock_guard<std::mutex> l(tables.lock);
        auto it = tables.slots.find(group);
        if (it != tables.slots.end()) {
            have_slots = true;
            slot_teams = it->second;
        }
    }
    ct::StartPlan plan;
    if (year) {
        if (!have_slots) {
            rep.result = "refused";
            summary = "the data group has no slot table yet (set_slots)";
            return SWOFTY_REFUSED;
        }
        if (!labels()) {
            rep.result = "failed";
            summary = "the teams table is not loaded yet";
            return SWOFTY_ASSET_LATE;
        }
        plan = ct::start_plan(group, ct::seasons_of(swofty_core::registry::store(), group), slot_rows_of(slot_teams));
    }
    const ct::StartYear *want = year ? plan.find(year) : nullptr;
    StartState was = take_start(group), now;
    now.year = year;
    size_t dressed = 0, undressed = 0, kept = 0;
    auto wanted = [&](const StartWorn &w) {
        if (!want)
            return false;
        for (auto &d : want->dresses)
            if (d.slot == w.slot && d.team == w.team)
                return true;
        return false;
    };
    for (auto &w : was.worn) {
        if (wanted(w))
            continue;
        std::string row = ct::identity_id(w.row);
        if (ct::dresser() && ct::dresser()(group.c_str(), w.slot, row.c_str(), row.c_str(), w.stat))
            ++undressed;
    }
    if (want) {
        for (auto &d : want->dresses) {
            auto i = static_cast<size_t>(d.slot - 1);
            int64_t stat = i < slot_teams.size() ? static_cast<int64_t>(slot_teams[i]) : -1;
            std::string row = ct::identity_id(d.row), team = ct::identity_id(d.team);
            bool before = std::any_of(was.worn.begin(), was.worn.end(),
                                      [&](const StartWorn &w) { return w.slot == d.slot && w.team == d.team; });
            if (ct::dresser() && ct::dresser()(group.c_str(), d.slot, row.c_str(), team.c_str(), stat)) {
                now.worn.push_back({d.slot, stat, d.row, d.team});
                ++(before ? kept : dressed);
                swofty::log(
                    "[career-ops] apply_teams slot %d: team row %s (stat id %lld) wears %s in the new-career wizard of %d",
                    d.slot, d.row.c_str(), static_cast<long long>(stat), team.c_str(), year);
            } else {
                rep.refusals.push_back("slot " + std::to_string(d.slot) + ": team row " + d.row + " (stat id " +
                                       std::to_string(stat) + ") could not be dressed as " + team +
                                       " yet: the loaded team record or the identity is not reachable");
            }
        }
        for (auto &n : want->notes)
            swofty::log("[career-ops]   %s", n.c_str());
    }
    if (!now.worn.empty()) {
        std::lock_guard<std::mutex> l(tables.lock);
        tables.starts[group] = now;
    }
    rep.changed = dressed + kept;
    rep.refused = rep.refusals.size();
    rep.extra = ",\"dressed\":" + std::to_string(dressed + kept) + ",\"undressed\":" + std::to_string(undressed);
    if (!year)
        summary =
            std::to_string(undressed) + " slot row(s) back in their own names and colours; the new-career wizard's dress is off";
    else if (!want)
        summary =
            "no career.season record of " + std::to_string(year) + " names an added team" +
            (undressed ? "; " + std::to_string(undressed) + " slot row(s) back in their own names and colours" : std::string());
    else
        summary = std::to_string(dressed) + " slot row(s) dressed, " + std::to_string(kept) + " still dressed, " +
                  std::to_string(undressed) + " undressed: Team Select lists the " + std::to_string(year) +
                  " added team(s) on their slot rows";
    return SWOFTY_ASSET_OK;
}

int api_apply_teams(SwoftyText request, char *out, size_t cap, size_t *len)
{
    namespace ct = career::ops::teams;
    Report rep;
    std::string summary, head = "apply_teams";
    int r = SWOFTY_ASSET_OK;
    json::Value j;
    std::string text = request.data ? std::string(request.data, request.size) : std::string();
    ct::Plan tp;
    size_t dressed = 0, dress_pending = 0;
    bool parsed = !on_flow_thread() ? false : json::parse(text, j) && j.type == json::Value::Obj;
    if (parsed && j["start"].type == json::Value::Obj) {
        r = start_teams(j["start"], rep, head, summary);
        swofty::log("[career-ops] %s: %s", head.c_str(), summary.c_str());
        for (auto &why : rep.refusals)
            swofty::log("[career-ops]   refused: %s", why.c_str());
        if (rep.result == "ok" && r != SWOFTY_ASSET_OK)
            rep.result = "failed";
        int w = put(rep.json(), out, cap, len);
        return w == SWOFTY_TOO_SMALL && r == SWOFTY_ASSET_OK ? SWOFTY_TOO_SMALL : r;
    }
    if (!on_flow_thread()) {
        rep.result = "failed";
        summary = "late: called off the flow thread";
        r = SWOFTY_ASSET_LATE;
    } else if (!parsed || j["season"].type != json::Value::Obj) {
        rep.result = "refused";
        summary = "the request is not a JSON object with a \"season\" record";
        r = SWOFTY_ASSET_INVALID;
    } else {
        const json::Value &s = j["season"];
        std::string group = s["data_group"].str();
        ct::SeasonIn cur;
        cur.year = number(s["year"], 0);
        for (auto &t : s["teams"].a) {
            ct::SlotIn x;
            x.slot = number(t["slot"], 0);
            x.team = t["team"].str();
            for (size_t k = 0; k < static_cast<size_t>(ct::kSeats) && k < t["seats"].a.size(); ++k)
                x.seats[k] = t["seats"].a[k].str();
            x.car = number(t["car"], -1);
            cur.slots.push_back(x);
        }
        head = "apply_teams " + std::to_string(cur.year) + " (season, " + (group.empty() ? "no data group" : group) + ")";
        std::vector<ct::SeasonIn> all = ct::seasons_of(swofty_core::registry::store(), group);
        all.erase(std::remove_if(all.begin(), all.end(), [&](const ct::SeasonIn &x) { return x.year == cur.year; }), all.end());
        all.push_back(cur);
        bool my_team = false;
        if (const SwoftyCareer *c = runtime::career_table(); c && c->state) {
            SwoftyCareerState st{};
            if (c->state(&st) == SWOFTY_ASSET_OK)
                my_team = st.mode == SWOFTY_CAREER_MY_TEAM;
        }
        ct::Table table = ct::build_table(std::move(all), my_team);
        std::vector<uint32_t> slot_teams;
        bool have_slots = false;
        {
            std::lock_guard<std::mutex> l(tables.lock);
            auto it = tables.slots.find(group);
            if (it != tables.slots.end()) {
                have_slots = true;
                slot_teams = it->second;
            }
        }
        if (have_slots && labels()) {
            std::vector<std::string> physical;
            std::lock_guard<std::mutex> l(tables.lock);
            for (uint32_t stat : slot_teams) {
                auto name = tables.labels.team_by_stat.find(stat);
                physical.push_back(name == tables.labels.team_by_stat.end() ? std::string() : name->second);
            }
            ct::anchor_rows(table, physical);
        }
        tp = ct::plan_season(table, cur);
        std::string missing;
        uintptr_t e = 0;
        std::vector<Row> rows;
        if (!tp.ok()) {
            rep.result = "refused";
            rep.refusals = tp.refusals;
            summary = "refused: " + (tp.refusals.empty() ? std::string("no plan") : tp.refusals.front());
            r = SWOFTY_REFUSED;
        } else if (!have_slots || slot_teams.size() < static_cast<size_t>(table.count())) {
            rep.result = "refused";
            summary = "the slot table of data group " + (group.empty() ? std::string("(none)") : group) + " has " +
                      std::to_string(slot_teams.size()) + " slot(s), the records need " + std::to_string(table.count());
            r = SWOFTY_REFUSED;
        } else if (!lineup_bound(missing)) {
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
            career::ops::plan::Input in;
            in.data_group = group;
            in.have_slots = true;
            in.slot_teams = slot_teams;
            std::map<int, int> car_of_team;
            for (auto &a : tp.actions) {
                if (a.kind == ct::Kind::closed)
                    continue;
                if (a.car >= 0)
                    car_of_team[static_cast<int>(slot_teams[static_cast<size_t>(a.slot - 1)])] = a.car;
                auto cs = std::find_if(cur.slots.begin(), cur.slots.end(), [&](const ct::SlotIn &x) { return x.slot == a.slot; });
                career::ops::plan::Team team;
                team.slot = a.slot;
                for (size_t k = 0; k < static_cast<size_t>(ct::kSeats); ++k) {
                    team.refs[k] = cs->seats[k];
                    team.drivers[k] = team.refs[k].empty() ? -1 : resolve_driver(team.refs[k]);
                }
                in.teams.push_back(team);
            }
            career::ops::plan::Plan p = career::ops::plan::plan(in, plan_rows(rows));
            std::vector<ct::SeatRow> seat_rows;
            for (auto &x : plan_rows(rows))
                seat_rows.push_back({stat_of(x.driver), x.team, x.position, x.car, x.human});
            ct::Closing closing = ct::close_slots(tp, slot_teams, seat_rows);
            Lineup l;
            l.year = cur.year;
            l.why = "season record";
            if (j["lineup"].type == json::Value::Obj) {
                std::string err;
                if (!read_lineup(j["lineup"], l, err)) {
                    rep.result = "refused";
                    summary = err;
                    r = SWOFTY_ASSET_INVALID;
                }
                l.year = cur.year;
                if (l.why.empty())
                    l.why = "season record";
            }
            if (r == SWOFTY_ASSET_OK) {
                if (l.player_team < 0)
                    l.player_team = p.player_team;
                std::vector<career::ops::plan::Given> given;
                for (auto &m : l.moves)
                    given.push_back({m.driver, m.team, m.seat});
                for (auto &m : l.repositions)
                    for (auto &x : plan_rows(rows))
                        if (career::ops::plan::stat_of(x.driver) == m.driver && x.car)
                            given.push_back({m.driver, x.team, m.seat});
                career::ops::plan::defer_to(p, given);
                for (auto &m : p.moves) {
                    auto c = car_of_team.find(m.team);
                    l.moves.push_back({m.driver, m.team, m.seat, c == car_of_team.end() ? -1 : c->second, -1, -1});
                }
                for (uint32_t d : p.releases)
                    if (std::find(l.releases.begin(), l.releases.end(), d) == l.releases.end())
                        l.releases.push_back(d);
                for (uint32_t d : closing.releases)
                    if (std::find(l.releases.begin(), l.releases.end(), d) == l.releases.end())
                        l.releases.push_back(d);
                rep.refusals = p.refusals;
                rep.refusals.insert(rep.refusals.end(), closing.refusals.begin(), closing.refusals.end());
                rep.refused = rep.refusals.size();
                StartState wizard = take_start(group);
                for (auto &a : tp.actions) {
                    int64_t row_stat = static_cast<int64_t>(slot_teams[static_cast<size_t>(a.slot - 1)]);
                    std::string row = ct::identity_id(a.row);
                    if (a.kind == ct::Kind::closed) {
                        if (ct::dresser())
                            ct::dresser()(group.c_str(), a.slot, row.c_str(), row.c_str(), row_stat);
                        continue;
                    }
                    if (a.kind == ct::Kind::open) {
                        if (ct::dresser())
                            ct::dresser()(group.c_str(), a.slot, row.c_str(), row.c_str(), row_stat);
                        continue;
                    }
                    std::string team = ct::identity_id(a.team);
                    bool from_wizard = std::any_of(wizard.worn.begin(), wizard.worn.end(), [&](const StartWorn &w) {
                        return w.slot == a.slot && w.stat == row_stat && ct::identity_id(w.team) == team;
                    });
                    if (ct::dresser() && ct::dresser()(group.c_str(), a.slot, row.c_str(), team.c_str(), row_stat)) {
                        ++dressed;
                        swofty::log("[career-ops] apply_teams slot %d: team row %s (stat id %lld) is dressed as %s%s", a.slot,
                                    a.row.c_str(), static_cast<long long>(row_stat), team.c_str(),
                                    from_wizard ? " (the same row and dress as the new-career wizard's)" : "");
                    } else {
                        ++dress_pending;
                        rep.refusals.push_back("slot " + std::to_string(a.slot) + ": team row " + a.row + " (stat id " +
                                               std::to_string(row_stat) + ") could not be dressed as " + team +
                                               " yet: the loaded team record or the identity is not reachable");
                    }
                }
                rep.refused = rep.refusals.size();
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
        }
        rep.extra = ",\"slots\":" + std::to_string(tp.actions.size()) + ",\"open\":" + std::to_string(tp.count(ct::Kind::open)) +
                    ",\"closed\":" + std::to_string(tp.count(ct::Kind::closed)) + ",\"dressed\":" + std::to_string(dressed) +
                    ",\"dress_pending\":" + std::to_string(dress_pending);
    }
    rep.refused = std::max(rep.refused, rep.refusals.size());
    swofty::log("[career-ops] %s: %s; %zu seat(s) set, %zu unchanged, %zu released, %zu refused, %zu dressed%s; %s", head.c_str(),
                tp.actions.empty() ? "no plan" : ct::summary(tp).c_str(), rep.changed, rep.unchanged, rep.released, rep.refused,
                dressed, dress_pending ? (", " + std::to_string(dress_pending) + " waiting for the team applier").c_str() : "",
                summary.c_str());
    for (auto &why : rep.refusals)
        swofty::log("[career-ops]   refused: %s", why.c_str());
    if (rep.result == "ok" && r != SWOFTY_ASSET_OK)
        rep.result = "failed";
    int w = put(rep.json(), out, cap, len);
    return w == SWOFTY_TOO_SMALL && r == SWOFTY_ASSET_OK ? SWOFTY_TOO_SMALL : r;
}

}
