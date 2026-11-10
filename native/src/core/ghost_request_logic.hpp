#pragma once

#include <swofty/json.hpp>

#include <string>

// swofty\tt\ghost\play.json, written by the launcher wrapper: {"lap": "<.swlap or .trace>", "label": "<text>", "driver":
// "<name>", "team": "<team>", "mode": "ghost" | "replay"}. mode defaults to ghost. "replay" asks for the game's own replay
// viewer; the viewer replays the session's recorded world buffer, not a trace (FINDINGS, Replay viewer), so the core plays
// the lap as the personal best ghost car in slot 3 and the owner opens the viewer on that car.
namespace runtime::ghost_request {

enum class Mode { ghost, replay };

struct Request {
    std::string lap, label, driver, team;
    Mode mode = Mode::ghost;
};

enum class Status { ok, not_object, no_lap, bad_mode };

inline Status parse(const std::string &text, Request &out)
{
    out = Request{};
    auto json = swofty::json::parse(text);
    if (!json.is_object())
        return Status::not_object;
    out.lap = std::string(json["lap"].as_string());
    out.label = std::string(json["label"].as_string());
    out.driver = std::string(json["driver"].as_string());
    out.team = std::string(json["team"].as_string());
    if (out.lap.empty())
        return Status::no_lap;
    std::string mode(json["mode"].as_string());
    if (mode.empty() || mode == "ghost")
        out.mode = Mode::ghost;
    else if (mode == "replay")
        out.mode = Mode::replay;
    else
        return Status::bad_mode;
    if (out.label.empty())
        out.label = out.driver;
    return Status::ok;
}

inline int target_slot(Mode m, int env_slot, bool rival_bound)
{
    if (m == Mode::replay)
        return 3;
    if (env_slot >= 0)
        return env_slot;
    return rival_bound ? 4 : 3;
}

inline const char *status_name(Status s)
{
    switch (s) {
    case Status::ok: return "ok";
    case Status::not_object: return "play.json is not a JSON object";
    case Status::no_lap: return "play.json has no lap";
    case Status::bad_mode: return "mode is ghost or replay";
    }
    return "?";
}

}
