#pragma once

#include "core/apply/content/lower.hpp"

#include <cstddef>
#include <string>
#include <utility>
#include <vector>

// Rev lights in equal-performance sessions. The steering wheel LEDs and the HUD rev bar take their thresholds from an
// entrants.engines row (revLightsStart, revLightsRedStart, revLightsPurpleStart, revLightsEnd, revFlashSpeed,
// revLightWarning). The game picks the row per car when it builds the wheel LEDs at session load
// (Car_SteeringWheelLeds_BuildEntries, 0x142f408b0): a car running equal performance (CarLive +0x12c == 1) takes the
// current game setup mode's equal_performance_engine (GameSetupMode +0xfa, set flag +0xfb), and only the others take
// their vehicle's engine. Every stock mode names Mercedes, the 2025 row (9,800 to 12,200 rpm), so a Time Trial with an
// 18,000 rpm V8 showed the 2025 lights. Every addon box shares the swofty_f1 host mode, so the era picker writes the
// pressed box's engine into that mode's equal_performance_engine, like its DRS group, and puts the game's back when
// another box or a stock box is pressed. The box's engine is the `engine` most of its vehicles name (rev_lights_pick).
// Pure, for the host tests.

namespace swofty_core::apply::content {

struct RevLightsPick {
    std::string package;
    std::string engine;    // an entrants.engines label; empty: the mode keeps its own engine
    std::size_t cars = 0;  // vehicles that name `engine`
    std::size_t named = 0; // vehicles that name any engine
    std::string line;
};

// The engine whose rev lights the box whose entrant list was pressed shows in equal-performance sessions: of the last
// content.package (in the order given) with that entrant list, the `engine` most of its teams' vehicle records name,
// the first of those in team order on a tie. Vehicles that name no engine keep their donor's and count for none.
inline RevLightsPick rev_lights_pick(const std::string &entrant_list,
                                     const std::vector<std::pair<std::string, const Value *>> &packages, const Lookup &lookup)
{
    RevLightsPick p;
    const Value *found = nullptr;
    for (auto &[id, rec] : packages)
        if (rec && (*rec)["entrant_list"].is_string() && (*rec)["entrant_list"].as_string() == entrant_list) {
            p.package = id;
            found = rec;
        }
    if (!found) {
        p.line = "box " + entrant_list + ": no content.package; swofty_f1 keeps its own equal-performance engine";
        return p;
    }
    std::vector<std::pair<std::string, std::size_t>> engines;
    const Value &teams = (*found)["teams"];
    for (std::size_t i = 0; teams.is_array() && i < teams.size(); i++) {
        const Value &vid = teams[i]["vehicle"];
        const Value *veh = vid.is_string() && lookup ? lookup("vehicle", std::string(vid.as_string())) : nullptr;
        if (!veh || !(*veh)["engine"].is_string())
            continue;
        std::string engine = key_of((*veh)["engine"].as_string());
        if (engine.empty())
            continue;
        p.named++;
        bool seen = false;
        for (auto &e : engines)
            if (e.first == engine) {
                e.second++;
                seen = true;
            }
        if (!seen)
            engines.push_back({engine, 1});
    }
    for (auto &e : engines)
        if (e.second > p.cars) {
            p.engine = e.first;
            p.cars = e.second;
        }
    if (p.engine.empty()) {
        p.line = "box " + entrant_list + " (" + p.package +
                 "): no vehicle names an engine; swofty_f1 keeps its own equal-performance engine";
        return p;
    }
    p.line = "box " + entrant_list + " (" + p.package + "): equal-performance sessions show the rev lights of engine " +
             p.engine + " (" + std::to_string(p.cars) + " of " + std::to_string(p.named) + " vehicle engine(s))";
    return p;
}

}
