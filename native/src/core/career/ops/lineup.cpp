#include "core/career/ops/lineup.hpp"

#include <algorithm>
#include <cstdio>
#include <map>
#include <mutex>

#include "core/career/ops/driver_rows.hpp"
#include "core/career/ops/tables.hpp"

namespace career::ops::live {

std::string quote(const std::string &s)
{
    std::string out = "\"";
    for (unsigned char c : s) {
        if (c == '"' || c == '\\') {
            out += '\\';
            out += char(c);
        } else if (c < 0x20) {
            char b[8];
            std::snprintf(b, sizeof b, "\\u%04x", c);
            out += b;
        } else {
            out += char(c);
        }
    }
    return out + "\"";
}

int put(const std::string &text, char *out, size_t cap, size_t *len)
{
    if (len)
        *len = text.size();
    if (!out && !cap)
        return SWOFTY_ASSET_OK;
    if (!out || cap < text.size() + 1)
        return SWOFTY_TOO_SMALL;
    std::memcpy(out, text.c_str(), text.size() + 1);
    return SWOFTY_ASSET_OK;
}

int number(const json::Value &v, int def) { return v.type == json::Value::Num ? static_cast<int>(v.n) : def; }

bool read_lineup(const json::Value &j, Lineup &l, std::string &err)
{
    if (j.type != json::Value::Obj) {
        err = "\"lineup\" is not an object";
        return false;
    }
    l.year = number(j["year"], 0);
    l.why = j["why"].str();
    l.visuals = j["visuals"].type == json::Value::Bool && j["visuals"].b;
    l.player_team = number(j["player_team"]);
    auto stat = [&](const json::Value &v, const char *what) -> int {
        int s = number(v);
        if (s < 0 || s >= 0x3fff)
            err = std::string(what) + ": a stat id must be 0..16382";
        return s;
    };
    for (auto &m : j["moves"].a) {
        int d = stat(m["driver"], "moves[].driver"), t = stat(m["team"], "moves[].team"), k = number(m["seat"]);
        if (k < 0 || k > 1)
            err = "moves[].seat must be 0 or 1";
        l.moves.push_back({static_cast<uint32_t>(d), t, k, number(m["car"]), number(m["holder"]), number(m["mate"])});
    }
    for (auto &m : j["repositions"].a) {
        int d = stat(m["driver"], "repositions[].driver"), k = number(m["seat"]);
        if (k < 0 || k > 1)
            err = "repositions[].seat must be 0 or 1";
        l.repositions.push_back({static_cast<uint32_t>(d), k});
    }
    for (auto &v : j["releases"].a)
        l.releases.push_back(static_cast<uint32_t>(stat(v, "releases[]")));
    for (auto &v : j["numbers"].a) {
        int n = number(v["number"]);
        if (n < 0 || n > 0xff)
            err = "numbers[].number must be 0..255";
        l.numbers.push_back({static_cast<uint32_t>(stat(v["driver"], "numbers[].driver")), n});
    }
    for (auto &v : j["cars"].a)
        l.cars.push_back({static_cast<uint32_t>(stat(v["driver"], "cars[].driver")), stat(v["car"], "cars[].car")});
    for (auto &v : j["looks"].a)
        l.looks.push_back({static_cast<uint32_t>(stat(v["driver"], "looks[].driver")), stat(v["look"], "looks[].look")});
    return err.empty();
}

int row_index(const std::vector<Row> &rows, uint32_t stat, uint32_t domain)
{
    std::vector<career::ops::rows::Row> view;
    view.reserve(rows.size());
    for (auto &r : rows)
        view.push_back({driver_of(r.rec), teamless(r.rec)});
    return career::ops::rows::pick(view, stat, domain);
}

int holder_of(const std::vector<Row> &rows, uint32_t team, int seat)
{
    for (size_t i = 0; i < rows.size(); ++i)
        if (has_car(rows[i].rec) && !teamless(rows[i].rec) && stat_of(rows[i].rec.team_stat_id) == team &&
            position_of(rows[i].rec) == seat)
            return static_cast<int>(i);
    return -1;
}

struct Write {
    int row = -1;
    EntrantWriteData data{};
    uint16_t position = 0;
    uint8_t number = kNoNumber;
    bool move = false, release = false;
    uint32_t old_team = kNoKey, new_team = kNoKey;
    std::string why;
};

EntrantRecord expected(const Row &r, const Write &w)
{
    EntrantRecord x = r.rec;
    x.driver_customisation = w.data.driver_customisation;
    x.team_stat_id = w.data.team_stat_id;
    x.vehicle_ref = w.data.vehicle_ref;
    x.driver_reg_value = w.data.driver_reg_value;
    x.vehicle_customisation = w.data.vehicle_customisation;
    x.badge_stat_id = w.data.badge_stat_id;
    x.const_ffff0012 = w.data.seat_value;
    x.team_position = w.position;
    x.car_number = w.number;
    return x;
}

int named_or_holder(const std::vector<Row> &rows, int named, uint32_t team, int seat)
{
    if (named >= 0) {
        int idx = row_index(rows, static_cast<uint32_t>(named));
        if (idx >= 0)
            return idx;
    }
    return holder_of(rows, team, seat);
}

bool seat_template(const std::vector<Row> &rows, const Row &driver, uint32_t team, int seat, int car, int holder, int mate_of,
                   EntrantWriteData &data, uint16_t &position, std::string &why)
{
    int h = named_or_holder(rows, holder, team, seat);
    if (h >= 0) {
        data = seat_data(driver, rows[h]);
        position = rows[h].rec.team_position;
        return true;
    }
    const Row *tpl = nullptr;
    int mate = named_or_holder(rows, mate_of, team, 1 - seat);
    if (mate >= 0)
        tpl = &rows[mate];
    for (size_t i = 0; !tpl && i < rows.size(); ++i)
        if (!teamless(rows[i].rec) && stat_of(rows[i].rec.team_stat_id) == team)
            tpl = &rows[i];
    data = tpl ? seat_data(driver, *tpl) : released_data(driver);
    if (!tpl)
        data.team_stat_id = team << 16;
    if (!tpl || !has_car(tpl->rec)) {
        if (car < 0) {
            why = "no row of its team has a car and the request names no car";
            return false;
        }
        data.vehicle_ref = static_cast<uint32_t>(car) << 16;
    }
    position = static_cast<uint16_t>(0x100 | seat);
    return true;
}

std::string seat_text(uint32_t team, int seat) { return team_name(team) + " seat " + std::to_string(seat + 1); }

int execute(const Lineup &p, Report &rep, std::string &summary)
{
    std::string missing;
    if (!lineup_bound(missing)) {
        rep.result = "failed";
        summary = "unavailable: binding(s) not resolved: " + missing;
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    uintptr_t e = entity();
    std::vector<Row> rows;
    if (!e || !entrant_rows(e, rows)) {
        rep.result = "failed";
        summary = "no career entity or its entrant rows are unreadable";
        return SWOFTY_ASSET_LATE;
    }
    uintptr_t manager = component("k_Career_DriverCareerContractsManagerComponentDesc");
    if (!manager) {
        rep.result = "failed";
        summary = "no DriverCareerContractsManagerComponent (not a Driver Career)";
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    std::vector<uintptr_t> objects;
    if (!contract_objects(manager, objects)) {
        rep.result = "failed";
        summary = "the contract list is unreadable";
        return SWOFTY_ASSET_LATE;
    }
    labels();
    std::map<int, Write> writes;
    auto write_for = [&](int idx) -> Write & {
        auto it = writes.find(idx);
        if (it != writes.end())
            return it->second;
        Write w;
        w.row = idx;
        w.data = own_data(rows[idx]);
        w.position = rows[idx].rec.team_position;
        w.number = rows[idx].rec.car_number;
        return writes.emplace(idx, w).first->second;
    };
    for (auto &m : p.moves) {
        int idx = row_index(rows, m.driver);
        if (idx < 0) {
            rep.result = "refused";
            summary = "driver " + driver_name(m.driver) + " has no single row";
            return SWOFTY_ASSET_INVALID;
        }
        EntrantWriteData data{};
        uint16_t position = 0;
        std::string bad;
        if (!seat_template(rows, rows[idx], static_cast<uint32_t>(m.team), m.seat, m.car, m.holder, m.mate, data, position,
                           bad)) {
            rep.result = "refused";
            summary = seat_text(static_cast<uint32_t>(m.team), m.seat) + " is empty and cannot be written: " + bad;
            return SWOFTY_ASSET_INVALID;
        }
        Write &w = write_for(idx);
        int from = position_of(rows[idx].rec);
        w.data = data;
        w.position = position;
        w.move = true;
        w.old_team = rows[idx].rec.team_stat_id;
        w.new_team = data.team_stat_id;
        w.why = (teamless(rows[idx].rec) || !has_car(rows[idx].rec) ? std::string("free agent")
                                                                    : seat_text(stat_of(rows[idx].rec.team_stat_id), from)) +
                " -> " + seat_text(static_cast<uint32_t>(m.team), m.seat) +
                (named_or_holder(rows, m.holder, static_cast<uint32_t>(m.team), m.seat) < 0 ? " (an empty seat)" : "");
    }
    for (auto &m : p.repositions) {
        int idx = row_index(rows, m.driver);
        if (idx < 0)
            continue;
        Write &w = write_for(idx);
        w.position = static_cast<uint16_t>(0x100 | m.seat);
        w.why = "race seat position " + std::to_string(position_of(rows[idx].rec)) + " -> " + std::to_string(m.seat);
    }
    for (uint32_t d : p.releases) {
        int idx = row_index(rows, d);
        if (idx < 0)
            continue;
        Write &w = write_for(idx);
        w.data = released_data(rows[idx]);
        w.position = 0;
        w.release = true;
        w.old_team = rows[idx].rec.team_stat_id;
        w.why = "released";
    }
    for (auto &n : p.numbers) {
        int idx = row_index(rows, n.driver);
        if (idx >= 0 && !human(rows[idx].rec))
            write_for(idx).number = static_cast<uint8_t>(n.value);
    }
    if (p.visuals) {
        for (auto &c : p.cars) {
            int idx = row_index(rows, c.driver);
            if (idx < 0 || c.value < 0)
                continue;
            Write &w = write_for(idx);
            w.data.vehicle_ref = (static_cast<uint32_t>(c.value) << 16) | (w.data.vehicle_ref & 0xffffu);
        }
        for (auto &l : p.looks) {
            int idx = row_index(rows, l.driver);
            if (idx >= 0 && l.value > 0)
                write_for(idx).data.driver_customisation = static_cast<uint32_t>(l.value) << 16;
        }
    }
    std::vector<int> order;
    for (auto &[idx, w] : writes) {
        EntrantRecord x = expected(rows[idx], w);
        if (std::memcmp(&rows[idx].rec, &x, sizeof x) != 0)
            order.push_back(idx);
        else
            ++rep.unchanged;
    }
    std::stable_sort(order.begin(), order.end(), [&](int a, int b) {
        auto rank = [&](int i) { return writes[i].move ? 0 : writes[i].release ? 1 : 2; };
        return rank(a) < rank(b);
    });
    std::vector<std::string> warnings;
    auto warn = [&](const std::string &s) { warnings.push_back(s); };
    auto sign_seats = [&]() {
        std::vector<Row> now;
        std::vector<uintptr_t> live;
        size_t need = 0;
        if (!entrant_rows(e, now) || !contract_objects(manager, live)) {
            warn("contracts: the line-up or contract list is unreadable");
            return;
        }
        for (auto &r : now) {
            if (!has_car(r.rec) || teamless(r.rec))
                continue;
            uint32_t d = driver_of(r.rec);
            uintptr_t own = contract_of(live, d);
            if (contract_fits(own, r.rec.team_stat_id))
                continue;
            if (human(r.rec)) {
                int at = static_cast<int>(&r - now.data());
                bool moved_player = writes.count(at) && writes[at].move;
                g::DriverContract k{};
                bool stale = own && mem::read(own, k) && k.active;
                if (!moved_player && !stale)
                    continue;
                if (p.player_team < 0 || static_cast<int>(stat_of(r.rec.team_stat_id)) != p.player_team) {
                    warn("player contract left alone: " + driver_name(stat_of(d)) + " sits at " +
                         team_name(stat_of(r.rec.team_stat_id)) + ", which is not the seat the line-up gives him (" +
                         (p.player_team < 0 ? std::string("no seat") : team_name(static_cast<uint32_t>(p.player_team))) + ")");
                    continue;
                }
            }
            ++need;
            Signing s;
            if (const char *why = plan_signing(manager, live, d, r.rec.team_stat_id, s))
                warn(driver_name(stat_of(d)) + " has no contract with " + team_name(stat_of(r.rec.team_stat_id)) + ": " + why);
            else if (!sign_contract(e, manager, s))
                warn(driver_name(stat_of(d)) + "'s contract with " + team_name(stat_of(r.rec.team_stat_id)) + " was not signed");
            else
                ++rep.contracts;
        }
        if (need && rep.contracts != need)
            warn("contracts: " + std::to_string(rep.contracts) + "/" + std::to_string(need) + " signed");
    };
    auto finish = [&](const std::string &head) {
        summary = head;
        for (auto &w : warnings)
            swofty::log("[career-ops]   WARNING %s", w.c_str());
    };
    if (order.empty()) {
        sign_seats();
        finish("nothing to write (" + std::to_string(rep.unchanged) + " row(s) already as planned)");
        return SWOFTY_ASSET_OK;
    }
    std::string rows_text;
    for (int idx : order) {
        const Write &w = writes[idx];
        rows_text += (rows_text.empty() ? "" : "; ") +
                     std::string(w.move      ? "move "
                                 : w.release ? "release "
                                             : "update ") +
                     driver_name(stat_of(driver_of(rows[idx].rec))) + (w.why.empty() ? "" : " " + w.why);
    }
    Baseline baseline(rows);
    std::vector<int> written;
    auto back = [&]() {
        size_t ok = 0;
        for (auto it = written.rbegin(); it != written.rend(); ++it) {
            std::vector<Row> now;
            const Row *r = entrant_rows(e, now) && now.size() > static_cast<size_t>(*it) ? &now[*it] : nullptr;
            if (r && write_row(e, *r, own_data(rows[*it]), rows[*it].rec.team_position, rows[*it].rec.car_number))
                ++ok;
        }
        warn("wrote back " + std::to_string(ok) + "/" + std::to_string(written.size()) + " row(s)" +
             (ok == written.size() ? "" : "; RELOAD THE SAVE without saving"));
        written.clear();
    };
    for (int idx : order) {
        const Write &w = writes[idx];
        std::vector<Row> now;
        if (!entrant_rows(e, now) || now.size() != rows.size() || now[idx].at != rows[idx].at) {
            back();
            rep.result = "failed";
            finish("FAILED: the line-up changed during the batch");
            return SWOFTY_ASSET_INVALID;
        }
        uint32_t d = driver_of(rows[idx].rec);
        bool found = false;
        if (w.release && !release_contract(e, manager, d, found))
            warn(driver_name(stat_of(d)) + ": contract not released");
        if ((w.move || w.release) &&
            !announce(manager, d, w.old_team, w.release ? kNoKey : w.new_team, w.release ? kReasonReleased : kReasonSigned))
            warn(driver_name(stat_of(d)) + ": MessageDriverMoveConfirmed not posted");
        if (!write_row(e, now[idx], w.data, w.position, w.number)) {
            back();
            rep.result = "failed";
            finish("FAILED writing " + driver_name(stat_of(d)));
            return SWOFTY_ASSET_INVALID;
        }
        written.push_back(idx);
    }
    std::vector<Row> after;
    bool ok = entrant_rows(e, after) && after.size() == rows.size();
    for (size_t i = 0; ok && i < rows.size(); ++i) {
        auto it = writes.find(static_cast<int>(i));
        bool mine = it != writes.end() && std::find(written.begin(), written.end(), static_cast<int>(i)) != written.end();
        const EntrantRecord &got = after[i].rec;
        if (!mine) {
            if (std::memcmp(&got, &rows[i].rec, sizeof got) != 0) {
                warn(driver_name(stat_of(driver_of(rows[i].rec))) + " changed though it was not written");
                ok = false;
            }
            continue;
        }
        EntrantRecord want = expected(rows[i], it->second);
        if (got.team_stat_id != want.team_stat_id || got.vehicle_ref != want.vehicle_ref ||
            got.driver_customisation != want.driver_customisation || got.car_number != want.car_number ||
            driver_of(got) != driver_of(want)) {
            warn(driver_name(stat_of(driver_of(rows[i].rec))) + " did not read back as planned");
            ok = false;
        }
    }
    if (!ok) {
        back();
        rep.result = "failed";
        finish("FAILED: the batch did not read back as planned; " + rows_text);
        return SWOFTY_ASSET_INVALID;
    }
    rep.written = written.size();
    for (int idx : written)
        rep.released += writes[idx].release;
    sign_seats();
    auto check = reinterpret_cast<g::Career_DriverNumbers_CheckExisting_t *>(bind("Career_DriverNumbers_CheckExisting"));
    if (uintptr_t cn = car_numbers(e, manager)) {
        std::vector<Row> now;
        if (check && entrant_rows(e, now))
            for (auto &r : now)
                if (has_car(r.rec) && r.rec.car_number == kNoNumber && !entity_busy(e) && unique_identity(e, r)) {
                    check(reinterpret_cast<void *>(cn), driver_of(r.rec));
                    ++rep.numbers;
                }
    }
    bool fin = true;
    if (!p.moves.empty() || !p.releases.empty()) {
        fin = finalise(e, baseline, rep.changes);
        rep.finalise = fin ? "posted" : "failed";
        if (!fin)
            rep.result = "failed";
    }
    finish(std::to_string(written.size()) + " row(s) written, " + std::to_string(rep.contracts) + " contract(s) signed, " +
           std::to_string(rep.numbers) + " number(s) drawn, finalise " + rep.finalise + " (" + std::to_string(rep.changes) +
           " line-up change(s)): " + rows_text);
    return fin ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

std::vector<career::ops::plan::Row> plan_rows(const std::vector<Row> &rows)
{
    std::vector<career::ops::plan::Row> out;
    for (auto &r : rows)
        out.push_back({driver_of(r.rec), teamless(r.rec) ? -1 : static_cast<int>(stat_of(r.rec.team_stat_id)), position_of(r.rec),
                       has_car(r.rec) && !teamless(r.rec), human(r.rec)});
    return out;
}

bool read_season(const json::Value &s, career::ops::plan::Input &in, int &year, std::string &err)
{
    if (s.type != json::Value::Obj) {
        err = "\"season\" is not an object";
        return false;
    }
    in.data_group = s["data_group"].str();
    year = number(s["year"], 0);
    {
        std::lock_guard<std::mutex> l(tables.lock);
        auto it = tables.slots.find(in.data_group);
        in.have_slots = it != tables.slots.end();
        if (in.have_slots)
            in.slot_teams = it->second;
    }
    for (auto &t : s["teams"].a) {
        career::ops::plan::Team team;
        team.slot = number(t["slot"], 0);
        const json::Value &seats = t["seats"];
        for (size_t k = 0; k < career::ops::plan::kSeats && k < seats.a.size(); ++k) {
            team.refs[k] = seats.a[k].str();
            team.drivers[k] = team.refs[k].empty() ? -1 : resolve_driver(team.refs[k]);
        }
        in.teams.push_back(team);
    }
    return true;
}

}
