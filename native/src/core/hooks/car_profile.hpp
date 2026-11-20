#pragma once
#include <algorithm>
#include <cmath>
#include <functional>
#include <cstdint>
#include <string>
#include <vector>

#include <swofty/json.hpp>
#include <swofty/systems/cars.hpp>

#include "core/hooks/car_live.hpp"
#include "core/hooks/context.hpp"

// car.profile records applied as core-owned layers (standard library 1.6, experimental). A profile names a
// scope (the selectors in `within`, the session types in `when`, the cars in `cars`, ai_only and player_only)
// and the values to layer on the cars it matches: the scales below become layers on the live fields of the same
// name, and every key the record does not name is a live field path (a number sets it, or an object with set,
// scale or add). The core drops its profile layers for a car and adds the matching profiles' again at car.built,
// car.reset, the upgrade replay and when the session changes. Profiles compose by priority, then by record id.
// The entries of `within` all have to hold, so a Grand Prix box and its Time Trial box go in one entry
// ({"content": [gp, tt]}); unreachable() names a within whose content entries no box holds at once.
// Ops<Tag> keeps the state per tag, so the host test runs the same code on fake cars.

namespace swofty_core::hooks::car_profile {

constexpr const char *OWNER = "car.profile";

struct Entry {
    std::string id;
    swofty::cars::Profile profile;
};

struct Spec {
    const char *path;
    uint32_t op;
    float value;
};

inline bool parse(const swofty::json::Value &value, swofty::cars::Profile &out, std::string *why)
{
    std::string text = swofty::json::dump(value);
    if (!swofty::from_json(text, out)) {
        if (why)
            *why = "not a car.profile record";
        return false;
    }
    return true;
}

inline std::vector<Spec> specs(const swofty::cars::Profile &p, std::vector<std::string> &keep)
{
    std::vector<Spec> out;
    auto scale = [&](const char *path, float v) {
        if (v != 1.0f)
            out.push_back({path, SWOFTY_CAR_OP_SCALE, v});
    };
    scale("engine.power_scale", p.power_scale);
    if (p.limiter_offset_rpm != 0.0f)
        out.push_back({"engine.limiter_offset_rpm", SWOFTY_CAR_OP_ADD, p.limiter_offset_rpm});
    scale("aero.lift_scale", p.lift_scale);
    scale("aero.drag_scale", p.drag_scale);
    scale("tyres.front.grip_scale", p.grip_scale);
    scale("tyres.rear.grip_scale", p.grip_scale);
    scale("ai.grip_scale", p.ai_grip_scale);
    if (auto o = p.extra.if_object()) {
        keep.reserve(keep.size() + o->size());
        for (const auto &[name, v] : *o) {
            keep.push_back(name);
            const char *path = keep.back().c_str();
            if (v.is_number()) {
                out.push_back({path, SWOFTY_CAR_OP_SET, float(v.as_double())});
            } else if (auto fields = v.if_object()) {
                for (const auto &[op, n] : *fields) {
                    if (!n.is_number())
                        continue;
                    uint32_t code = op == "scale" ? SWOFTY_CAR_OP_SCALE
                                    : op == "add" ? SWOFTY_CAR_OP_ADD
                                    : op == "set" ? SWOFTY_CAR_OP_SET
                                                  : 99u;
                    if (code != 99u)
                        out.push_back({path, code, float(n.as_double())});
                }
            }
        }
    }
    return out;
}

inline swofty::json::Value within_list(const swofty::json::Value &within)
{
    if (within.is_array())
        return within;
    swofty::json::Value list = swofty::json::Value::array();
    if (!within.is_null())
        list.push_back(within);
    return list;
}

inline bool within_matches(const swofty::cars::Profile &p, const Facts &facts, std::string *why)
{
    if (p.within.is_null())
        return true;
    Selectors sel;
    if (!Selectors::compile(swofty::json::dump(within_list(p.within)), sel, why))
        return false;
    return sel.match(facts);
}

struct ContentNeed {
    std::vector<std::string> ids;
    bool exact = false;
};

inline std::vector<ContentNeed> content_needs(const swofty::json::Value &within)
{
    std::vector<ContentNeed> out;
    swofty::json::Value list = within_list(within);
    for (const auto &entry : *list.if_array()) {
        auto o = entry.if_object();
        if (!o)
            continue;
        for (const auto &[key, v] : *o) {
            if (key != "content" && key != "content_exact")
                continue;
            ContentNeed n;
            n.exact = key == "content_exact";
            if (auto s = v.if_string())
                n.ids.push_back(*s);
            else if (auto a = v.if_array())
                for (const auto &x : *a)
                    if (auto s = x.if_string())
                        n.ids.push_back(*s);
            if (!n.ids.empty())
                out.push_back(std::move(n));
        }
    }
    return out;
}

inline bool box_holds(const std::vector<std::string> &chain, const ContentNeed &n)
{
    for (const auto &id : n.ids)
        if (n.exact ? !chain.empty() && chain.front() == id : std::find(chain.begin(), chain.end(), id) != chain.end())
            return true;
    return false;
}

inline std::string unreachable(const swofty::json::Value &within,
                               const std::function<std::vector<std::vector<std::string>>()> &boxes)
{
    auto needs = content_needs(within);
    if (needs.size() < 2)
        return {};
    for (const auto &chain : boxes()) {
        bool all = true;
        for (const auto &n : needs)
            all = all && box_holds(chain, n);
        if (all)
            return {};
    }
    std::string named, ids;
    for (size_t i = 0; i < needs.size(); i++) {
        std::string one;
        for (const auto &id : needs[i].ids) {
            one += (one.empty() ? "" : " or ") + id;
            ids += (ids.empty() ? "\"" : ", \"") + id + "\"";
        }
        named += (i == 0 ? "" : i + 1 == needs.size() ? " and " : ", ") + one;
    }
    return "its within entries all have to hold, and no box is " + named + " at once; one entry {\"content\": [" + ids +
           "]} matches any of them";
}

enum class Match { yes, no, needs_identity };

inline Match matches(const swofty::cars::Profile &p, const std::string &session_type, bool within, bool is_ai, uint32_t index)
{
    if (!within)
        return Match::no;
    if (!p.when.empty()) {
        bool hit = false;
        for (auto t : p.when)
            hit = hit || session_type == swofty::events::detail::session_type_name(t);
        if (!hit)
            return Match::no;
    }
    if (p.ai_only && !is_ai)
        return Match::no;
    if (p.player_only && is_ai)
        return Match::no;
    if (!p.cars.indices.empty() && std::find(p.cars.indices.begin(), p.cars.indices.end(), index) == p.cars.indices.end())
        return Match::no;
    if (!p.cars.vehicle_package.empty() || !p.cars.vehicle.empty() || !p.cars.team.empty())
        return Match::needs_identity;
    return Match::yes;
}

struct Memo {
    struct Key {
        uint64_t store = 0, epoch = 0, tick = 0;
        std::string session;
        bool ai = false;
        bool operator==(const Key &o) const
        {
            return store == o.store && epoch == o.epoch && tick == o.tick && ai == o.ai && session == o.session;
        }
    };
    static constexpr uint32_t CARS = car_live::MAX_CARS;
    Key keys[CARS];
    bool have[CARS] = {};

    bool fresh(uint32_t index, const Key &k) const { return index < CARS && have[index] && keys[index] == k; }
    void note(uint32_t index, const Key &k)
    {
        if (index < CARS) {
            keys[index] = k;
            have[index] = true;
        }
    }
    void forget(uint32_t index)
    {
        if (index < CARS)
            have[index] = false;
    }
    void clear() { std::fill(std::begin(have), std::end(have), false); }
};

struct Report {
    size_t layers = 0, matched = 0;
    std::vector<std::string> needs_identity, refused;
};

template <class Ops> struct Applier {
    static Report evaluate(std::vector<Entry> entries, const std::string &session_type,
                           const std::function<bool(const Entry &)> &within, uint32_t index, bool is_ai)
    {
        Report r;
        std::stable_sort(entries.begin(), entries.end(), [](const Entry &a, const Entry &b) {
            return a.profile.priority != b.profile.priority ? a.profile.priority < b.profile.priority : a.id < b.id;
        });
        Ops::drop_owner(OWNER, index);
        for (const auto &e : entries) {
            Match m = matches(e.profile, session_type, !within || within(e), is_ai, index);
            if (m == Match::needs_identity) {
                r.needs_identity.push_back(e.id);
                continue;
            }
            if (m == Match::no)
                continue;
            r.matched++;
            std::vector<std::string> keep;
            for (const Spec &s : specs(e.profile, keep)) {
                const swofty_core::hooks::car_live::Field *f = swofty_core::hooks::car_live::find_field_path(s.path);
                if (!f || Ops::add_layer(OWNER, index, f->id, s.op, s.value, e.profile.priority, nullptr) != SWOFTY_ASSET_OK)
                    r.refused.push_back(e.id + ": " + s.path);
                else
                    r.layers++;
            }
        }
        return r;
    }
};

}
