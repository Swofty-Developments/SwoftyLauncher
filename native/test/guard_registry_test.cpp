#include "core/guards/order.hpp"
#include "core/runtime/loader/core_set_order.hpp"

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
namespace loader = runtime::loader;

// The core mixin sets in the order offer_core_sets and settle_core_sets in runtime/manager.cpp listed them on
// v1.10.0 (9d21d3a9b) before the registry took them over: 1.9.63's lists (native/src/core/manager.cpp), which do not
// depend on link order, with v1.10.0's loading set after audio_queue.
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
    "weather_chain",
    "resume_gate",
    "tyre_curve",
    "anim_clip_time",
    "marshal_lights",
    "anim_packet_time",
    "speech",
    "vehicle_override",
    "phys_nonfinite",
    "ai_marker_cap",
    "collision_pool",
    "box_videos",
    "f1_world_tiles",
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
    "box_videos",
    "quali_tyres",
    "gp_continue",
    "track_list_guard",
    "garage_setup",
    "theatre_list",
    "tile_list_select",
    "session_enter",
    "ai_line_set",
    "weather_chain",
    "resume_gate",
    "tyre_curve",
    "anim_clip_time",
    "marshal_lights",
    "anim_packet_time",
    "speech",
    "vehicle_override",
    "phys_nonfinite",
    "ai_marker_cap",
    "collision_pool",
    "f1_world_tiles",
};

// The order 1.9.63 armed the hook guards in, each from its own SWOFTY_ON_LOAD entry, as the armed lines of a 1.9.63
// session's loader.log show it, with the AI ERS and calendar track guards where 1.9.76's link order armed them. The SDK lists on-load entries in the order they are constructed, and the core's static
// constructors run in reverse link order: lld lays __CTOR_LIST__ out in link order and the mingw crt calls it from the
// last entry (gccmain.c __do_global_ctors). So 1.9.63 armed them in the reverse of its build.sh link order. A hook
// guard added since links after the listed sources (tools/core_link_order.txt), so it arms first.
const std::vector<std::string> HOOKED = {
    "pit_garage_state", "gp_weekend_track",  "rnd_suppliers",   "replay_read", "ai_curves",
    "ai_ers",           "calendar_track",    "teamhq_track",    "setup_vm",    "teammate_pairs",
    "race_retirement",  "sequencer_records", "activity_resume",
};

int failures = 0;

void expect(bool ok, const std::string &what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what.c_str());
        failures++;
    }
}

// The core's sources in link order, as native/tools/build_graph.py links them: those tools/core_link_order.txt lists,
// in its order, then every other source under src/core, sorted by path.
std::vector<std::filesystem::path> link_order(const std::filesystem::path &core)
{
    std::filesystem::path native = core.parent_path().parent_path();
    std::vector<std::string> listed;
    std::ifstream in(native / "tools/core_link_order.txt");
    for (std::string line; std::getline(in, line);)
        if (!line.empty() && line[0] != '#' && line.rfind("src/core/", 0) == 0)
            listed.push_back(line);
    std::vector<std::string> rest;
    for (auto &entry : std::filesystem::recursive_directory_iterator(core)) {
        if (entry.path().extension() != ".cpp")
            continue;
        std::string rel = "src/core/" + std::filesystem::relative(entry.path(), core).generic_string();
        if (std::find(listed.begin(), listed.end(), rel) == listed.end())
            rest.push_back(rel);
    }
    std::sort(rest.begin(), rest.end());
    std::vector<std::filesystem::path> out;
    for (auto *names : {&listed, &rest})
        for (auto &rel : *names)
            out.push_back(native / rel);
    return out;
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
    expect(loader::CORE_SET_COUNT == OFFERED.size(), "the registry holds " + std::to_string(OFFERED.size()) + " sets");
    for (size_t i = 0; i < OFFERED.size() && i < loader::CORE_SET_COUNT; i++)
        expect(loader::core_set_name(loader::CoreSetId(i)) == OFFERED[i],
               "offer slot " + std::to_string(i) + " is " + OFFERED[i]);

    expect(loader::COMMIT_ORDER.size() == SETTLED.size(), "the commit order holds " + std::to_string(SETTLED.size()));
    for (size_t i = 0; i < SETTLED.size() && i < loader::COMMIT_ORDER.size(); i++)
        expect(loader::core_set_name(loader::COMMIT_ORDER[i]) == SETTLED[i],
               "commit slot " + std::to_string(i) + " is " + SETTLED[i]);
    for (size_t i = 0; i < loader::CORE_SET_COUNT; i++) {
        auto id = loader::CoreSetId(i);
        expect(loader::settles(id) == (loader::commit_rank(id) < loader::COMMIT_ORDER.size()),
               std::string(loader::core_set_name(id)) + " settles exactly when the commit order lists it");
    }

    // Every set registers exactly once, in some core source, under its id.
    std::filesystem::path core = argc > 1 ? argv[1] : "../src/core";
    std::map<std::string, size_t> registrations;
    for (auto &entry : std::filesystem::recursive_directory_iterator(core)) {
        if (entry.path().extension() != ".cpp")
            continue;
        std::string text = read(entry.path());
        for (size_t i = 0; i < loader::CORE_SET_COUNT; i++) {
            std::string name(loader::CORE_SET_NAMES[i]);
            registrations[name] += count(text, "CoreSetId::" + name);
        }
    }
    for (auto &name : OFFERED)
        expect(registrations[name] == 1, name + " registers once (found " + std::to_string(registrations[name]) + ")");

    // Every hook guard registers once, and their sources' on-load entries run in HookGuardId order: the k-th on-load
    // entry the SDK runs from a hook guard's source arms the k-th hook guard, which is the guard that source declares.
    // The entries run in reverse link order.
    expect(guards::HOOK_GUARD_COUNT == HOOKED.size(), "the registry holds " + std::to_string(HOOKED.size()) + " hook guards");
    for (size_t i = 0; i < HOOKED.size() && i < guards::HOOK_GUARD_COUNT; i++)
        expect(guards::hook_guard_name(guards::HookGuardId(i)) == HOOKED[i],
               "hook guard slot " + std::to_string(i) + " is " + HOOKED[i]);
    std::vector<std::string> run;
    auto linked = link_order(core);
    for (auto it = linked.rbegin(); it != linked.rend(); ++it) {
        const std::filesystem::path &path = *it;
        std::string text = read(path);
        expect(!text.empty(), path.string() + " is readable");
        for (auto &name : HOOKED) {
            size_t n = count(text, "SWOFTY_HOOK_GUARD(" + name) + count(text, "SWOFTY_MID_HOOK_GUARD(" + name);
            if (n)
                run.push_back(name);
            expect(n <= 1, name + " registers once in " + path.string());
        }
    }
    expect(run == HOOKED, "the hook guards' on-load entries run (in reverse link order) in HookGuardId order, each once");

    if (failures)
        return 1;
    std::printf("guard registry: %zu core sets in the starting offer and commit order and %zu hook guards in their arming "
                "order, each registered once\n",
                OFFERED.size(), HOOKED.size());
    return 0;
}
