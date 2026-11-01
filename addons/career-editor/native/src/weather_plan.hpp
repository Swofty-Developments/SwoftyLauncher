// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: the weather plan model (protocol/weather.md). Parses one session's plan, turns it
// into the per-state arrays Sess_Tod_BuildSessionDataset_Thunk takes, and clamps planned intensities
// into the game's per-type range. No game access, so native/test builds it on the host.

#pragma once
#include <cstdint>
#include <string>
#include <vector>

#include "ce.hpp"

namespace ce::weather {

constexpr int kMaxPlanStates = 6;
constexpr float kMinRampS = 1.0f;
constexpr float kMinHoldS = 1.0f;
inline const char *const kStates[6] = {"clear", "light_cloud", "overcast", "light_rain", "heavy_rain", "storm"};

enum Unit { kFraction, kMinute, kLap };
inline const char *const kUnitKey[3] = {"at_fraction", "at_minute", "at_lap"};

struct Point {
    double at = 0;
    int state = 0;
    double intensity = -1;
    double ramp = -1;
};

struct SessionPlan {
    int unit = kFraction;
    std::vector<Point> pts;
    int start = -1;
    int laps = 0;
    int minutes = 0;
    bool has_air = false, has_track = false;
    double air = 0, track = 0;
};

// One state per entry; inten < 0 leaves the state's intensity to the builder's draw.
struct Build {
    uint32_t n = 0;
    uint32_t dropped = 0;
    uint8_t type[kMaxPlanStates] = {};
    float hold[kMaxPlanStates] = {};
    float trans[kMaxPlanStates] = {};
    float inten[kMaxPlanStates] = {-1, -1, -1, -1, -1, -1};
};

// WeatherTypeRange intensity_min / intensity_max of one weather type.
struct TypeRange {
    float min = 0, max = 0;
};

bool timed_session(const std::string &key);
int state_of(const std::string &s);
bool is_int(const Json &v, double lo, double hi);
bool is_num(const Json &v, double lo, double hi);
bool at_zero(const SessionPlan &p, double at);
int plan_states(const SessionPlan &p);
bool parse_session(const Json &v, const char *key, SessionPlan &p, std::string &err);
Build make_build(const SessionPlan &p, float len, int game_first);

// "" when every hold, transition and intensity is finite and in range and the states fit the session;
// otherwise what is wrong. The write site refuses a build that fails this instead of handing it to the game.
std::string validate_build(const Build &b, float len);

// Moves each planned intensity into its type's range, or back to the builder's draw when that range is not
// a usable 0..1 interval. Returns one line per changed state.
std::vector<std::string> clamp_build(Build &b, const TypeRange (&ranges)[6]);

} // namespace ce::weather
