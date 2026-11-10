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
    std::string lap, label, driver, team, track, content, weather;
    Mode mode = Mode::ghost;
    int64_t expires = 0;
};

enum class Status { ok, not_object, no_lap, bad_mode, no_track };

inline std::string lower(std::string s)
{
    for (char &c : s)
        if (c >= 'A' && c <= 'Z')
            c = char(c - 'A' + 'a');
    return s;
}

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
    out.track = lower(std::string(json["track"].as_string()));
    out.content = lower(std::string(json["content"].as_string()));
    out.weather = lower(std::string(json["weather"].as_string()));
    out.expires = json["expires"].as_int(0);
    if (out.lap.empty())
        return Status::no_lap;
    std::string mode(json["mode"].as_string());
    if (out.track.empty())
        return Status::no_track;
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

struct Session {
    bool time_trial = false;
    std::string content, track, weather;
};

enum class Verdict { ok, expired, not_time_trial, wrong_track, wrong_content, wrong_weather };

constexpr int64_t MAX_AGE_S = 30 * 60;

// Whether a read request may be armed in this session: on the exact board it names, before it expires (default: 30 minutes
// after it was read).
inline Verdict check(const Request &r, const Session &s, int64_t now_unix, int64_t read_unix)
{
    if (r.expires > 0 ? now_unix > r.expires : now_unix - read_unix > MAX_AGE_S)
        return Verdict::expired;
    if (!s.time_trial)
        return Verdict::not_time_trial;
    if (lower(s.track) != r.track)
        return Verdict::wrong_track;
    if (!r.content.empty() && lower(s.content) != r.content)
        return Verdict::wrong_content;
    if (!r.weather.empty() && !s.weather.empty() && lower(s.weather) != r.weather)
        return Verdict::wrong_weather;
    return Verdict::ok;
}

inline const char *verdict_name(Verdict v)
{
    switch (v) {
    case Verdict::ok: return "ok";
    case Verdict::expired: return "expired";
    case Verdict::not_time_trial: return "the session is not a Time Trial";
    case Verdict::wrong_track: return "the session's track is not the request's";
    case Verdict::wrong_content: return "the session's content is not the request's";
    case Verdict::wrong_weather: return "the session's weather is not the request's";
    }
    return "?";
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
    case Status::no_track: return "play.json has no track (legacy requests are discarded)";
    }
    return "?";
}

}
