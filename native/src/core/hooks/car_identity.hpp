#pragma once
#include <algorithm>
#include <functional>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <vector>

#include <swofty/json.hpp>
#include <swofty/systems/cars.hpp>

// The references a live car answers to in car.profile's cars selectors (car_profile.hpp), from what its session
// entrant holds: the database row labels of its team, vehicle, vehicle package and driver, and the team.identity a
// dressed career slot wears.
// - A team answers to game/<row label>, to each team.identity record whose row it is (<addon>/<key>, the key is the
//   row's label) or the identity a dressed slot wears, to the constructor/ ids those identities are or provide and the
//   team.constructor records that name them, and to the key of each team entry of the active box's content.package
//   chain whose row it is ("mercedes").
// - A vehicle answers to game/<row label> and the vehicle records with that label, a vehicle package to
//   game/<row label> and the vehicle.package records of that key, a driver to game/<row label>, the driver.identity
//   records with that label and their person/ ids.
// Source reads the registry; the host test hands it records. build() reads each registry once per store generation;
// a stock row's own identity (game/<label>) is read the first time a car needs it.
namespace swofty_core::hooks::car_identity {

using swofty::json::Value;

struct Source {
    std::function<std::vector<std::string>(const char *registry)> list;
    std::function<std::optional<Value>(const char *registry, const std::string &id)> get;
    // Whether the game database has a row of this label: what is "team", "vehicle", "driver" or "package". Null: every
    // label counts as there.
    std::function<bool(const char *what, const std::string &label)> row;
};

struct Seen {
    std::string team, worn, vehicle, package, driver;
};

struct Refs {
    std::vector<std::string> team, vehicle, vehicle_package, driver;
};

inline std::string key_of(const std::string &id)
{
    auto s = id.rfind('/');
    return s == std::string::npos ? id : id.substr(s + 1);
}

inline bool starts(const std::string &s, const char *prefix) { return s.rfind(prefix, 0) == 0; }

inline void add(std::vector<std::string> &to, const std::string &ref)
{
    if (!ref.empty() && std::find(to.begin(), to.end(), ref) == to.end())
        to.push_back(ref);
}

struct BoxTeam {
    std::string key, row;
};

struct Index {
    std::map<std::string, std::vector<std::string>> teams, vehicles, packages, drivers;
    std::map<std::string, std::vector<std::string>> constructors, persons;
    std::map<std::string, std::vector<BoxTeam>> boxes;
    std::set<std::string> team_ids, vehicle_ids, package_ids, driver_ids, constructor_ids, person_ids, box_keys;
    std::map<std::string, std::vector<std::string>> game_constructors, game_persons;
};

inline std::string text(const Value &v) { return v.is_string() ? std::string(v.as_string()) : std::string(); }

inline std::vector<std::string> identity_constructors(const Value &rec)
{
    std::vector<std::string> out;
    std::string c = text(rec["constructor"]);
    if (!c.empty())
        add(out, starts(c, "constructor/") ? c : "constructor/" + key_of(c));
    if (auto a = rec["provides"].if_array())
        for (const auto &p : *a)
            if (starts(text(p), "constructor/"))
                add(out, text(p));
    return out;
}

inline std::string record_label(const std::string &id, const Value &rec)
{
    std::string l = text(rec["label"]);
    return l.empty() ? key_of(id) : l;
}

inline Index build(const Source &src)
{
    Index ix;
    if (!src.list || !src.get)
        return ix;
    auto each = [&](const char *registry, auto f) {
        for (const auto &id : src.list(registry)) {
            if (starts(id, "game/"))
                continue;
            if (auto rec = src.get(registry, id))
                f(id, *rec);
        }
    };
    each("team.identity", [&](const std::string &id, const Value &rec) {
        ix.team_ids.insert(id);
        ix.teams[key_of(id)].push_back(id);
        for (const auto &c : identity_constructors(rec)) {
            add(ix.constructors[id], c);
            ix.constructor_ids.insert(c);
        }
    });
    for (const auto &id : src.list("team.constructor")) {
        ix.constructor_ids.insert(id);
        if (auto rec = src.get("team.constructor", id))
            if (std::string team = text((*rec)["team"]); !team.empty())
                add(ix.constructors[team], id);
    }
    std::map<std::string, std::string> constructor_team;
    for (const auto &[team, ids] : ix.constructors)
        for (const auto &c : ids)
            if (!constructor_team.count(c))
                constructor_team[c] = team;
    std::map<std::string, std::string> vehicle_label;
    each("vehicle", [&](const std::string &id, const Value &rec) {
        ix.vehicle_ids.insert(id);
        std::string l = record_label(id, rec);
        vehicle_label[id] = l;
        ix.vehicles[l].push_back(id);
    });
    for (const auto &id : src.list("vehicle.package")) {
        ix.package_ids.insert(id);
        if (!starts(id, "game/"))
            ix.packages[key_of(id)].push_back(id);
    }
    each("driver.identity", [&](const std::string &id, const Value &rec) {
        ix.driver_ids.insert(id);
        ix.drivers[record_label(id, rec)].push_back(id);
        if (std::string p = text(rec["person"]); !p.empty()) {
            add(ix.persons[id], p);
            ix.person_ids.insert(p);
        }
    });
    for (const auto &id : src.list("driver.person"))
        ix.person_ids.insert(id);
    for (const auto &id : src.list("content.package")) {
        auto rec = src.get("content.package", id);
        if (!rec)
            continue;
        auto teams = (*rec)["teams"].if_array();
        if (!teams)
            continue;
        for (const auto &t : *teams) {
            std::string key = text(t["key"]), team = text(t["team"]), vehicle = text(t["vehicle"]), row;
            if (key.empty())
                continue;
            if (starts(team, "constructor/")) {
                auto it = constructor_team.find(team);
                row = it == constructor_team.end() ? std::string() : key_of(it->second);
            } else if (!team.empty()) {
                row = key_of(team);
            } else if (!vehicle.empty()) {
                auto it = vehicle_label.find(vehicle);
                row = it == vehicle_label.end() ? key_of(vehicle) : it->second;
            }
            ix.box_keys.insert(key);
            if (!row.empty())
                ix.boxes[id].push_back({key, row});
        }
    }
    return ix;
}

inline const std::vector<std::string> &game_refs(std::map<std::string, std::vector<std::string>> &cache, const char *registry,
                                                 const std::string &id, const Source &src, bool team)
{
    auto it = cache.find(id);
    if (it != cache.end())
        return it->second;
    std::vector<std::string> out;
    if (src.get)
        if (auto rec = src.get(registry, id)) {
            if (team)
                out = identity_constructors(*rec);
            else if (std::string p = text((*rec)["person"]); !p.empty())
                out.push_back(p);
        }
    return cache[id] = std::move(out);
}

inline const std::vector<std::string> *find(const std::map<std::string, std::vector<std::string>> &m, const std::string &k)
{
    auto it = m.find(k);
    return it == m.end() ? nullptr : &it->second;
}

inline Refs refs_of(const Seen &s, const std::vector<std::string> &chain, Index &ix, const Source &src)
{
    Refs r;
    std::string label = s.worn.empty() ? s.team : key_of(s.worn);
    if (!s.worn.empty())
        add(r.team, s.worn);
    else if (!s.team.empty())
        add(r.team, "game/" + s.team);
    if (!label.empty())
        if (auto ids = find(ix.teams, label))
            for (const auto &id : *ids)
                add(r.team, id);
    std::vector<std::string> named = r.team;
    for (const auto &t : named) {
        if (auto c = find(ix.constructors, t))
            for (const auto &id : *c)
                add(r.team, id);
        if (starts(t, "game/"))
            for (const auto &id : game_refs(ix.game_constructors, "team.identity", t, src, true))
                add(r.team, id);
    }
    if (!label.empty())
        for (const auto &box : chain) {
            auto it = ix.boxes.find(box);
            if (it != ix.boxes.end())
                for (const auto &bt : it->second)
                    if (bt.row == label)
                        add(r.team, bt.key);
        }
    if (!s.vehicle.empty()) {
        add(r.vehicle, "game/" + s.vehicle);
        if (auto ids = find(ix.vehicles, s.vehicle))
            for (const auto &id : *ids)
                add(r.vehicle, id);
    }
    if (!s.package.empty()) {
        add(r.vehicle_package, "game/" + s.package);
        if (auto ids = find(ix.packages, s.package))
            for (const auto &id : *ids)
                add(r.vehicle_package, id);
    }
    if (!s.driver.empty()) {
        std::string game = "game/" + s.driver;
        add(r.driver, game);
        if (auto ids = find(ix.drivers, s.driver))
            for (const auto &id : *ids) {
                add(r.driver, id);
                if (auto p = find(ix.persons, id))
                    for (const auto &x : *p)
                        add(r.driver, x);
            }
        for (const auto &x : game_refs(ix.game_persons, "driver.identity", game, src, false))
            add(r.driver, x);
    }
    return r;
}

inline bool needs(const swofty::cars::CarSelect &c)
{
    return !c.team.empty() || !c.vehicle.empty() || !c.vehicle_package.empty() || !c.driver.empty();
}

inline bool any_in(const std::vector<std::string> &want, const std::vector<std::string> &have)
{
    for (const auto &w : want)
        if (std::find(have.begin(), have.end(), w) != have.end())
            return true;
    return false;
}

inline bool selects(const swofty::cars::CarSelect &c, const Refs &r)
{
    return (c.team.empty() || any_in(c.team, r.team)) && (c.vehicle.empty() || any_in(c.vehicle, r.vehicle)) &&
           (c.vehicle_package.empty() || any_in(c.vehicle_package, r.vehicle_package)) &&
           (c.driver.empty() || any_in(c.driver, r.driver));
}

// The entries of a cars selector that name nothing the registry or the game database has, one line each.
inline std::vector<std::string> unknown(const swofty::cars::CarSelect &c, const Index &ix, const Source &src)
{
    std::vector<std::string> out;
    auto has_row = [&](const char *what, const std::string &label) { return !src.row || src.row(what, label); };
    auto say = [&](const char *field, const std::string &name, const std::string &why) {
        out.push_back(std::string("cars.") + field + " \"" + name + "\" " + why);
    };
    for (const auto &t : c.team) {
        if (starts(t, "game/")) {
            if (!has_row("team", t.substr(5)))
                say("team", t, "names no team row of the game database");
        } else if (starts(t, "constructor/")) {
            if (!ix.constructor_ids.count(t))
                say("team", t, "names no team.constructor record, and no team is or provides it");
        } else if (t.find('/') != std::string::npos) {
            if (!ix.team_ids.count(t))
                say("team", t, "names no team.identity record");
        } else if (!ix.box_keys.count(t)) {
            say("team", t,
                "is no box's team key; a team is a box's team key, a team.identity id (<addon>/<key>), game/<row label> or "
                "constructor/<key>");
        }
    }
    for (const auto &v : c.vehicle) {
        if (starts(v, "game/") ? !has_row("vehicle", v.substr(5)) : !ix.vehicle_ids.count(v))
            say("vehicle", v, "names no vehicle record or game row; a vehicle is <addon>/<key> or game/<row label>");
    }
    for (const auto &p : c.vehicle_package) {
        if (starts(p, "game/") ? !has_row("package", p.substr(5)) : !ix.package_ids.count(p))
            say("vehicle_package", p,
                "names no vehicle.package record or game row; a package is <addon>/<key> or game/<row label>");
    }
    for (const auto &d : c.driver) {
        bool known = starts(d, "game/")     ? has_row("driver", d.substr(5))
                     : starts(d, "person/") ? ix.person_ids.count(d) > 0
                                            : ix.driver_ids.count(d) > 0;
        if (!known)
            say("driver", d,
                "names no driver.identity record, person or game row; a driver is <addon>/<key>, game/<row label> or "
                "person/<key>");
    }
    return out;
}

inline std::string joined(const std::vector<std::string> &v)
{
    std::string s;
    for (const auto &x : v)
        s += (s.empty() ? "" : ", ") + x;
    return s.empty() ? std::string("none") : s;
}

inline std::string describe(const Refs &r)
{
    return "team " + joined(r.team) + "; vehicle " + joined(r.vehicle) + "; vehicle package " + joined(r.vehicle_package) +
           "; driver " + joined(r.driver);
}

}
