#pragma once

#include "core/apply/content/boot.hpp"
#include "core/apply/content/race_fuel.hpp"
#include "core/apply/content/box_options.hpp"
#include "core/apply/content/grid_rules.hpp"
#include "core/apply/content/quali_tyres.hpp"
#include "core/apply/content/rev_lights.hpp"
#include "core/apply/content/tyre_change.hpp"
#include "core/apply/content/tyre_warmers.hpp"
#include "core/apply/content/weekend.hpp"
#include "core/apply/ids.hpp"
#include "core/apply/room.hpp"
#include "core/apply/scheduler.hpp"
#include "core/career/ops/teams_plan.hpp"
#include "core/career/ops/teams_store.hpp"
#include "core/guards/session/team_slots_logic.hpp"
#include "core/registry/table.hpp"
#include "core/registry/validate.hpp"

#include <cstdio>
#include <fstream>
#include <set>
#include <sstream>

using namespace swofty_core;
using swofty::json::Value;
namespace content = apply::content;
namespace rows = apply::content::rows;

inline int failures = 0;

inline void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

inline std::string slurp(const std::string &path)
{
    std::ifstream f(path, std::ios::binary);
    std::stringstream s;
    s << f.rdbuf();
    return s.str();
}

struct Db {
    std::map<uint64_t, rows::Rec> recs;
    void put(const std::string &path, int tag, const std::string &text) { recs[content::gvdb::fnv64(path)] = {tag, text}; }
    rows::Base base()
    {
        return [this](uint64_t key) -> std::optional<rows::Rec> {
            auto it = recs.find(key);
            if (it == recs.end())
                return std::nullopt;
            return it->second;
        };
    }
};

inline std::optional<std::string> written(const rows::Engine &e, const std::string &path)
{
    for (auto &w : e.writes())
        if (w.path == path)
            return w.text;
    return std::nullopt;
}

inline Value op(const char *json) { return swofty::json::parse(json); }

inline std::vector<std::string> holes(const std::vector<rows::Write> &applied, const rows::Base &base)
{
    std::unordered_map<uint64_t, std::string> have;
    for (auto &w : applied)
        have[w.key] = w.text;
    auto exists = [&](const std::string &path) {
        uint64_t k = content::gvdb::fnv64(path);
        return have.count(k) || base(k);
    };
    const std::string tail = "_instancecount_";
    std::vector<std::string> out;
    for (auto &w : applied) {
        std::size_t dot = w.path.rfind("._");
        if (dot == std::string::npos || w.path.size() < tail.size() ||
            w.path.compare(w.path.size() - tail.size(), tail.size(), tail) != 0)
            continue;
        std::string table = w.path.substr(0, dot);
        std::string row = w.path.substr(dot + 2, w.path.size() - dot - 2 - tail.size());
        auto stock = base(w.key);
        int64_t from = stock ? std::strtoll(stock->text.c_str(), nullptr, 10) : 0;
        for (int64_t i = from, n = std::strtoll(w.text.c_str(), nullptr, 10); i < n; i++) {
            std::string at = table + "." + row + "_" + std::to_string(i);
            if (!exists(at + "._metadata_") && !exists(at))
                out.push_back(at);
        }
    }
    return out;
}

inline std::vector<rows::Write> owned_by(const rows::Engine &e, const std::set<std::string> &owners)
{
    std::vector<rows::Write> out;
    for (auto &w : e.writes())
        if (owners.count(w.owner))
            out.push_back(w);
    return out;
}

void engine_tests();
void room_hole_tests();
void stock_package_rows(Db &db);
void leaderboard_tests(const std::string &regs);
void variant_tests(const std::string &regs);
void twelve_team_tests(const std::string &regs);
void short_list_tests(const std::string &regs);
void package_team_row_tests();
void garage_tests();
void sponsors_tests();
void box_assets_tests();
void livery_tests();
void stock_team_rows(Db &db);
void team_tests(const std::string &regs);
void twelve_slot_tests(const std::string &regs);
void slot_tests();
void seat_entrant_tests();
void box_time_trial_tests();
void drs_tests(const std::string &regs);
void race_fuel_tests();
void points_tests();
void career_registry_tests(const std::string &regs);
void shared_switch_tests(const std::string &regs);
void id_clash_tests(const std::string &regs);
void stock_only_tests(const std::string &regs);
void virtual_list_tests(const std::string &regs);
void setup_mode_cap_tests(const std::string &regs);
void late_write_tests(const std::string &regs);
void ahead_tests(const std::string &regs);
void career_rules_tests();
