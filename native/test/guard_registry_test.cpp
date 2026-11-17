#include "core/guards/order.hpp"

#include <algorithm>
#include <cctype>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

namespace guards = swofty_core::guards;

// The core mixin sets in the order offer_core_sets and settle_core_sets in runtime/manager.cpp listed them on
// v1.10.0 (9d21d3a9b) before the registry took them over.
const std::vector<std::string> OFFERED = {
    "save_sandbox",
    "career_content",
    "characters",
    "wizard",
    "tile_route",
    "era",
    "tt_guard",
    "career_alias",
    "car_category",
    "save_splice",
    "ers_newlap",
    "ers_mode",
    "car_levers",
    "garage_crew",
    "garage_placement",
    "pit_slots",
    "pit_box",
    "garage_slots",
    "chassis_registry",
    "standings_guard",
    "ai_corner_guard",
    "replay_grid_guard",
    "fe_tile_guard",
    "myteam_vehicle_guard",
    "myteam_vehicle_heal",
    "myteam_vehicle_keep",
    "tyre_slots",
    "tyre_models",
    "track_acclim",
    "audio_queue",
    "loading",
    "heap_sizes",
    "team_slots",
    "track_select",
    "leaderboard_guard",
    "lap_host",
    "thrustmaster_lcd",
    "estimate_laptime",
    "red_flag_recompute",
    "quali_tyres",
    "box_grid",
    "gp_continue",
    "track_list_guard",
    "watchdog_grace",
    "garage_setup",
    "theatre_list",
    "tile_list_select",
    "session_enter",
    "ai_line_set",
};

const std::vector<std::string> SETTLED = {
    "save_sandbox",
    "tile_route",
    "career_content",
    "characters",
    "wizard",
    "era",
    "tt_guard",
    "career_alias",
    "car_category",
    "save_splice",
    "ers_newlap",
    "lap_host",
    "ers_mode",
    "car_levers",
    "standings_guard",
    "fe_tile_guard",
    "myteam_vehicle_guard",
    "myteam_vehicle_heal",
    "myteam_vehicle_keep",
    "ai_corner_guard",
    "replay_grid_guard",
    "watchdog_grace",
    "tyre_slots",
    "tyre_models",
    "garage_crew",
    "pit_slots",
    "pit_box",
    "garage_slots",
    "chassis_registry",
    "track_acclim",
    "audio_queue",
    "loading",
    "heap_sizes",
    "team_slots",
    "track_select",
    "leaderboard_guard",
    "thrustmaster_lcd",
    "estimate_laptime",
    "red_flag_recompute",
    "box_grid",
    "quali_tyres",
    "gp_continue",
    "track_list_guard",
    "garage_setup",
    "theatre_list",
    "tile_list_select",
    "session_enter",
    "ai_line_set",
};

// The guards that armed themselves from their own SWOFTY_ON_LOAD entries before the registry, in the order the
// SDK listed those entries (the link order of their sources).
const std::vector<std::string> HOOKED = {
    "ai_curves", "activity_resume", "rnd_suppliers", "sequencer_records", "teamhq_track",
    "setup_vm",  "race_retirement", "replay_read",   "teammate_pairs",    "gp_weekend_track",
};

int failures = 0;

void expect(bool ok, const std::string &what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what.c_str());
        failures++;
    }
}

// The core's sources in link order: native/tools/build_graph.py walks each directory's files, sorted, before its
// subdirectories, sorted.
void link_order(const std::filesystem::path &dir, std::vector<std::filesystem::path> &out)
{
    std::vector<std::filesystem::path> files, dirs;
    for (auto &entry : std::filesystem::directory_iterator(dir))
        (entry.is_directory() ? dirs : files).push_back(entry.path());
    std::sort(files.begin(), files.end());
    std::sort(dirs.begin(), dirs.end());
    for (auto &f : files)
        if (f.extension() == ".cpp")
            out.push_back(f);
    for (auto &d : dirs)
        link_order(d, out);
}

std::string read(const std::filesystem::path &p)
{
    std::ifstream in(p, std::ios::binary);
    std::stringstream s;
    s << in.rdbuf();
    return s.str();
}

size_t count(const std::string &text, const std::string &needle)
{
    size_t n = 0;
    for (size_t at = text.find(needle); at != std::string::npos; at = text.find(needle, at + needle.size()))
        if (at + needle.size() >= text.size() ||
            !(std::isalnum((unsigned char)text[at + needle.size()]) || text[at + needle.size()] == '_'))
            n++;
    return n;
}

int main(int argc, char **argv)
{
    expect(guards::CORE_SET_COUNT == OFFERED.size(), "the registry holds " + std::to_string(OFFERED.size()) + " sets");
    for (size_t i = 0; i < OFFERED.size() && i < guards::CORE_SET_COUNT; i++)
        expect(guards::core_set_name(guards::CoreSetId(i)) == OFFERED[i],
               "offer slot " + std::to_string(i) + " is " + OFFERED[i]);

    expect(guards::COMMIT_ORDER.size() == SETTLED.size(), "the commit order holds " + std::to_string(SETTLED.size()));
    for (size_t i = 0; i < SETTLED.size() && i < guards::COMMIT_ORDER.size(); i++)
        expect(guards::core_set_name(guards::COMMIT_ORDER[i]) == SETTLED[i],
               "commit slot " + std::to_string(i) + " is " + SETTLED[i]);
    for (size_t i = 0; i < guards::CORE_SET_COUNT; i++) {
        auto id = guards::CoreSetId(i);
        expect(guards::settles(id) == (guards::commit_rank(id) < guards::COMMIT_ORDER.size()),
               std::string(guards::core_set_name(id)) + " settles exactly when the commit order lists it");
    }

    // Every set registers exactly once, in some core source, under its id.
    std::filesystem::path core = argc > 1 ? argv[1] : "../src/core";
    std::map<std::string, size_t> registrations;
    for (auto &entry : std::filesystem::recursive_directory_iterator(core)) {
        if (entry.path().extension() != ".cpp")
            continue;
        std::string text = read(entry.path());
        for (size_t i = 0; i < guards::CORE_SET_COUNT; i++) {
            std::string name(guards::CORE_SET_NAMES[i]);
            registrations[name] += count(text, "CoreSetId::" + name);
        }
    }
    for (auto &name : OFFERED)
        expect(registrations[name] == 1, name + " registers once (found " + std::to_string(registrations[name]) + ")");

    // Every hook guard registers once, and their sources still link in HookGuardId order, so the SDK's on-load entries
    // run at the position in the core's on-load order the guards' own entries had.
    expect(guards::HOOK_GUARD_COUNT == HOOKED.size(), "the registry holds " + std::to_string(HOOKED.size()) + " hook guards");
    for (size_t i = 0; i < HOOKED.size() && i < guards::HOOK_GUARD_COUNT; i++)
        expect(guards::hook_guard_name(guards::HookGuardId(i)) == HOOKED[i],
               "hook guard slot " + std::to_string(i) + " is " + HOOKED[i]);
    std::vector<std::filesystem::path> sources;
    link_order(core, sources);
    std::vector<std::string> linked;
    size_t first_hook = 0, last_hook = 0, on_load = 0;
    for (auto &path : sources) {
        std::string text = read(path);
        bool hook = false;
        for (auto &name : HOOKED) {
            size_t n = count(text, "SWOFTY_HOOK_GUARD(" + name);
            if (n)
                linked.push_back(name), hook = true;
            expect(n <= 1, name + " registers once in " + path.string());
        }
        if (text.find("SWOFTY_ON_LOAD\n") != std::string::npos || hook) {
            on_load++;
            if (hook && !first_hook)
                first_hook = on_load;
            if (hook)
                last_hook = on_load;
        }
    }
    expect(linked == HOOKED, "the hook guards' sources link in HookGuardId order, each once");
    expect(last_hook - first_hook + 1 == HOOKED.size(), "no other on-load entry links between the hook guards");

    if (failures)
        return 1;
    std::printf("guard registry: %zu core sets in the starting offer and commit order and %zu hook guards in their arming "
                "order, each registered once\n",
                OFFERED.size(), HOOKED.size());
    return 0;
}
