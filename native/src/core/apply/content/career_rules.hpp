#pragma once

#include "core/apply/content/lower.hpp"
#include "core/registry/store.hpp"

#include <deque>
#include <functional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

// The rules a career season races under (docs/std/systems/career.md#the-season-s-rules): the box its career.season record
// names. The season is the effective career.season record of the career's data group whose year is the career's year
// (the year career.season.start or the owner's career::report_season gave the apply scheduler). Its box is the first
// alternative of the record's box reference ("a|b": a, else b) that is a content.package. At each session load of the
// career, the era picker puts that package's rules into the career's current game setup mode, the way a press of the
// box puts them into swofty_f1 (picker/career_rules.cpp). A career whose data group has no season for its year, or
// whose season names a stock box, keeps the game's rules. Pure, for the host tests.

namespace swofty_core::apply::content {

struct CareerSeasonFacts {
    std::string career, data_group;
    int year = 0;
    bool safety_car = false; // SWOFTY_CAREER_SAFETY_CAR=1: the season's rules.safety_car runs (career_opts_in)
};

struct CareerBox {
    std::string season, package, entrant_list, line;
    std::vector<std::string> runs, left_out;
};

// The box rules a career season leaves out: rules.weekend_mode (a career's weekends are its calendar's and its season
// length's) and the four race settings, which stay as the player set them for the career.
inline constexpr const char *CAREER_LEFT_OUT[] = {"weekend_mode", "safety_car", "formation_lap", "penalties", "corner_cutting"};

// The left-out rules a career season runs when its facts opt in: rules.safety_car with CareerSeasonFacts::safety_car
// (SWOFTY_CAREER_SAFETY_CAR=1). The era picker then sets the options store's safety_car_frequency at each session
// load of the career, the option a box press sets, and puts the player's index back when the career unloads or a
// season without the rule loads.
inline bool career_opts_in(const CareerSeasonFacts &f, std::string_view rule) { return f.safety_car && rule == "safety_car"; }

// The box rules a career season runs: the ones the era picker writes into a setup mode or hands to the session hooks.
inline constexpr const char *CAREER_RULES[] = {"drs",
                                               "tyre_warmers",
                                               "mandatory_tyre_change",
                                               "race_time_limit_s",
                                               "points",
                                               "refuelling",
                                               "race_fuel_kg",
                                               "refuel_stint_kg",
                                               "quali_fuel_kg",
                                               "race_start_fuel_from_quali",
                                               "race_tyre_change",
                                               "grid",
                                               "qualifying_tyres"};

inline std::vector<std::string> box_alternatives(std::string_view ref)
{
    std::vector<std::string> out;
    while (!ref.empty()) {
        size_t bar = ref.find('|');
        std::string_view one = ref.substr(0, bar);
        while (!one.empty() && one.back() == '?')
            one.remove_suffix(1);
        if (!one.empty())
            out.emplace_back(one);
        if (bar == std::string_view::npos)
            break;
        ref.remove_prefix(bar + 1);
    }
    return out;
}

using PackageLookup = std::function<const Value *(const std::string &id)>;

inline CareerBox career_box_pick(const CareerSeasonFacts &f, const std::vector<std::pair<std::string, const Value *>> &seasons,
                                 const PackageLookup &package)
{
    CareerBox b;
    if (f.career.empty()) {
        b.line = "no career is loaded";
        return b;
    }
    std::string head = "career " + f.career;
    if (f.data_group.empty()) {
        b.line = head + " has no data group; the game's rules";
        return b;
    }
    if (!f.year) {
        b.line = head + " (data group " + f.data_group + "): the season's year is not known yet; the game's rules";
        return b;
    }
    head += " season " + std::to_string(f.year);
    const Value *found = nullptr;
    for (auto &[id, rec] : seasons) {
        if (!rec || (*rec)["data_group"].as_string() != f.data_group || !(*rec)["year"].is_int() ||
            (*rec)["year"].as_int() != f.year)
            continue;
        if (found) {
            b.line = head + ": career.season records " + b.season + " and " + id + " of data group " + f.data_group +
                     " are both for " + std::to_string(f.year) + "; the game's rules";
            b.season.clear();
            return b;
        }
        found = rec;
        b.season = id;
    }
    if (!found) {
        b.line = head + ": no career.season record of data group " + f.data_group + " is for " + std::to_string(f.year) +
                 "; the game's rules";
        return b;
    }
    head += " (" + b.season + ")";
    const Value &ref = (*found)["box"];
    if (!ref.is_string() || ref.as_string().empty()) {
        b.line = head + ": the season names no box; the game's rules";
        return b;
    }
    std::vector<std::string> alts = box_alternatives(ref.as_string());
    const Value *pkg = nullptr;
    for (auto &a : alts)
        if (package && (pkg = package(a)) != nullptr) {
            b.package = a;
            break;
        }
    if (!pkg) {
        b.line = head + ": box " + std::string(ref.as_string()) +
                 " is no content.package (a stock box, or its addon is off); the game's rules";
        return b;
    }
    if (!(*pkg)["entrant_list"].is_string() || (*pkg)["entrant_list"].as_string().empty()) {
        b.line = head + ": box " + b.package + " has no entrant list; the game's rules";
        b.package.clear();
        return b;
    }
    b.entrant_list = std::string((*pkg)["entrant_list"].as_string());
    const Value &rules = (*pkg)["rules"];
    for (const char *rule : CAREER_RULES)
        if (!rules[rule].is_null())
            b.runs.emplace_back(rule);
    for (const char *rule : CAREER_LEFT_OUT)
        if (!rules[rule].is_null())
            (career_opts_in(f, rule) ? b.runs : b.left_out).emplace_back(rule);
    auto joined = [](const std::vector<std::string> &v) {
        std::string out;
        for (auto &r : v)
            out += (out.empty() ? "" : ", ") + r;
        return out;
    };
    b.line = head + ": box " + b.entrant_list + " (" + b.package + "), " +
             (b.runs.empty() ? std::string("no rule a career runs") : "rules " + joined(b.runs)) +
             (b.left_out.empty() ? std::string() : "; left out in a career: " + joined(b.left_out));
    return b;
}

inline CareerSeasonFacts career_facts(const registry::Context &ctx, bool safety_car = false)
{
    return {ctx.career, ctx.data_group, ctx.has_year ? ctx.year : 0, safety_car};
}

inline CareerBox career_box(registry::Store &st, const registry::Context &ctx, bool safety_car = false)
{
    CareerSeasonFacts f = career_facts(ctx, safety_car);
    std::vector<std::pair<std::string, Value>> held;
    if (!f.career.empty() && !f.data_group.empty() && f.year)
        for (auto &id : st.list("career.season", "")) {
            registry::Record r = st.get_live("career.season", id, ctx);
            if (r.status == registry::Status::ok)
                held.emplace_back(id, std::move(r.value));
        }
    std::vector<std::pair<std::string, const Value *>> seasons;
    for (auto &[id, v] : held)
        seasons.emplace_back(id, &v);
    std::deque<Value> packages;
    PackageLookup lookup = [&](const std::string &id) -> const Value * {
        registry::Record r = st.get("content.package", id);
        if (r.status != registry::Status::ok)
            return nullptr;
        packages.push_back(std::move(r.value));
        return &packages.back();
    };
    return career_box_pick(f, seasons, lookup);
}

}
