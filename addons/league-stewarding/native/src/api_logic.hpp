// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

#include "http.hpp"
#include "telemetry.hpp"

namespace steward {

enum class Kind { warning, time, drive_through, stop_go, grid, dsq };

struct PenaltyAction {
    int car = -1;
    Kind kind = Kind::warning;
    int seconds = 5;
    int places = 3;
    int reason = 4;
    std::string text;
};

inline constexpr int kMaxCar = 25;
inline constexpr int kMaxSeconds = 300;
inline constexpr int kMaxPlaces = 20;
inline constexpr int kMaxReason = 0x3c;
inline constexpr int kReasonDefault = 4;

inline const char *kind_name(Kind k)
{
    switch (k) {
    case Kind::warning: return "warning";
    case Kind::time: return "time";
    case Kind::drive_through: return "drive_through";
    case Kind::stop_go: return "stop_go";
    case Kind::grid: return "grid";
    case Kind::dsq: return "dsq";
    }
    return "?";
}

inline bool parse_kind(const std::string &s, Kind &k)
{
    for (Kind c : {Kind::warning, Kind::time, Kind::drive_through, Kind::stop_go, Kind::grid, Kind::dsq})
        if (s == kind_name(c))
            return k = c, true;
    return false;
}

inline bool parse_penalty(const std::string &body, PenaltyAction &out, std::string &err)
{
    out = PenaltyAction{};
    double n = 0;
    if (!http::json_number(body, "car", n) || n < 0 || n > kMaxCar || n != double(int(n))) {
        err = "car must be a grid index from 0 to 25";
        return false;
    }
    out.car = int(n);
    std::string kind;
    if (!http::json_string(body, "kind", kind) || !parse_kind(kind, out.kind)) {
        err = "kind must be warning, time, drive_through, stop_go, grid or dsq";
        return false;
    }
    if (http::json_number(body, "seconds", n))
        out.seconds = int(n);
    if (http::json_number(body, "places", n))
        out.places = int(n);
    if (http::json_number(body, "infringement", n)) {
        if (n < 0 || n > kMaxReason) {
            err = "infringement must be an infringement type from 0 to 60";
            return false;
        }
        out.reason = int(n);
    }
    http::json_string(body, "reason", out.text);
    if (out.text.size() > 120)
        out.text.resize(120);
    if (out.kind == Kind::time && (out.seconds < 1 || out.seconds > kMaxSeconds)) {
        err = "seconds must be from 1 to 300";
        return false;
    }
    if (out.kind == Kind::grid && (out.places < 1 || out.places > kMaxPlaces)) {
        err = "places must be from 1 to 20";
        return false;
    }
    return true;
}

enum class RcAction { sc, vsc, end, red_flag };

inline const char *rc_action_name(RcAction a)
{
    switch (a) {
    case RcAction::sc: return "sc";
    case RcAction::vsc: return "vsc";
    case RcAction::end: return "end";
    case RcAction::red_flag: return "red_flag";
    }
    return "?";
}

inline bool parse_rc(const std::string &body, RcAction &out, std::string &err)
{
    std::string action;
    if (http::json_string(body, "action", action))
        for (RcAction a : {RcAction::sc, RcAction::vsc, RcAction::end, RcAction::red_flag})
            if (action == rc_action_name(a))
                return out = a, true;
    err = "action must be sc, vsc, end or red_flag";
    return false;
}

enum ScState {
    kScStart = 0,
    kScIdle = 1,
    kScAiTool = 2,
    kScDeploy = 3,
    kScGroup = 4,
    kScPacing = 5,
    kScReturn = 6,
    kScCinematicIntro = 7,
    kScCinematicOutroCountdown = 10,
    kScReturnedToPit = 11,
    kScVscDeploy = 12,
    kScVscEnding = 13,
    kScRedFlagIntro = 14,
    kScRedFlagDeploy = 15,
};

inline int rc_target(RcAction a, int from)
{
    switch (a) {
    case RcAction::sc:
        return from == kScIdle || from == kScVscDeploy || from == kScVscEnding ? int(kScDeploy) : -1;
    case RcAction::vsc:
        return from == kScIdle ? int(kScVscDeploy) : -1;
    case RcAction::end:
        if (from == kScDeploy || from == kScGroup || from == kScPacing)
            return kScReturn;
        return from == kScVscDeploy ? int(kScVscEnding) : -1;
    case RcAction::red_flag:
        switch (from) {
        case kScIdle: case kScDeploy: case kScGroup: case kScPacing: case kScReturn: case kScReturnedToPit:
        case kScVscDeploy: case kScVscEnding:
            return kScRedFlagIntro;
        default:
            return -1;
        }
    }
    return -1;
}

inline const char *rc_state_name(int s)
{
    switch (s) {
    case kScDeploy: case kScGroup: case kScPacing:
    case 7: case 8: case 9: case kScCinematicOutroCountdown:
        return "sc";
    case kScReturn: return "sc_ending";
    case kScVscDeploy: return "vsc";
    case kScVscEnding: return "vsc_ending";
    case kScRedFlagIntro: case kScRedFlagDeploy: return "red_flag";
    default: return "none";
    }
}

inline std::string rc_refusal(RcAction a, int from)
{
    if (from < 0)
        return "this session has no safety car vehicle, so race control cannot be driven";
    return std::string("race control is in state ") + rc_state_name(from) + ", which does not allow " + rc_action_name(a);
}

struct StateView {
    std::string role = "offline", phase = "unknown";
    std::string rc_state = "none";
    int rc_raw = 0;
    bool auto_penalties = true;
    uint16_t telemetry_port = 20777;
    Snapshot snap;
    std::vector<Event> local_events;
};

inline std::vector<Event> merge_events(const std::vector<Event> &a, const std::vector<Event> &b)
{
    std::vector<Event> out;
    out.reserve(a.size() + b.size());
    out.insert(out.end(), a.begin(), a.end());
    out.insert(out.end(), b.begin(), b.end());
    std::stable_sort(out.begin(), out.end(), [](const Event &x, const Event &y) { return x.t < y.t; });
    return out;
}

inline std::string state_json(const StateView &v)
{
    using http::json_escape;
    char num[64];
    std::string o = "{\"role\":\"" + v.role + "\",\"phase\":\"" + v.phase + "\"";
    o += ",\"track\":\"" + json_escape(v.snap.track) + "\",\"lap\":" + std::to_string(v.snap.lap) +
         ",\"total_laps\":" + std::to_string(v.snap.total_laps);
    o += ",\"race_control\":{\"state\":\"" + v.rc_state + "\",\"raw\":" + std::to_string(v.rc_raw) + "}";
    o += std::string(",\"auto_penalties\":") + (v.auto_penalties ? "true" : "false");
    o += std::string(",\"telemetry\":{\"receiving\":") + (v.snap.receiving ? "true" : "false") +
         ",\"port\":" + std::to_string(v.telemetry_port) + ",\"last_packet_age_s\":";
    if (v.snap.last_packet_age_s < 0)
        o += "null";
    else {
        std::snprintf(num, sizeof num, "%.2f", v.snap.last_packet_age_s);
        o += num;
    }
    o += "},\"drivers\":[";
    for (size_t i = 0; i < v.snap.drivers.size(); i++) {
        const Driver &d = v.snap.drivers[i];
        std::snprintf(num, sizeof num, "%.1f", double(d.penalty_s));
        o += (i ? "," : "");
        o += "{\"index\":" + std::to_string(d.index) + ",\"name\":\"" + json_escape(d.name) + "\",\"team\":\"" +
             json_escape(d.team) + "\",\"position\":" + std::to_string(d.position) + ",\"lap\":" + std::to_string(d.lap) +
             ",\"in_pit\":" + (d.in_pit ? "true" : "false") + ",\"human\":" + (d.human ? "true" : "false") +
             ",\"penalty_s\":" + num + ",\"warnings\":" + std::to_string(d.warnings) + "}";
    }
    o += "],\"events\":[";
    std::vector<Event> events = merge_events(v.snap.events, v.local_events);
    for (size_t i = 0; i < events.size(); i++) {
        const Event &e = events[i];
        std::snprintf(num, sizeof num, "%.2f", e.t);
        o += (i ? "," : "");
        o += std::string("{\"t\":") + num + ",\"code\":\"" + json_escape(e.code) + "\",\"text\":\"" + json_escape(e.text) +
             "\",\"car\":" + std::to_string(e.car) + ",\"other\":" + std::to_string(e.other) + "}";
    }
    o += "]}";
    return o;
}

}
