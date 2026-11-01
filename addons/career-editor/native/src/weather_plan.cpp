// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: the weather plan model (weather_plan.hpp).

#include "weather_plan.hpp"

#include <cmath>
#include <cstdio>

namespace ce::weather {

namespace {

double fraction(const SessionPlan &p, double at, float len)
{
    if (!std::isfinite(at))
        return 0;
    double f = p.unit == kFraction ? at
               : p.unit == kMinute ? (p.minutes ? at / p.minutes : at * 60.0 / len)
                                   : (at - 1.0) / (p.laps ? p.laps : 1);
    return f < 0 ? 0 : f > 1 ? 1 : f;
}

} // namespace

bool timed_session(const std::string &key)
{
    return key == "p1" || key == "p2" || key == "p3" || key == "q" || key == "ss";
}

int state_of(const std::string &s)
{
    for (int i = 0; i < 6; ++i)
        if (s == kStates[i])
            return i;
    return -1;
}

bool is_int(const ce::Json &v, double lo, double hi)
{
    return v.type == ce::Json::Num && std::isfinite(v.n) && std::floor(v.n) == v.n && v.n >= lo && v.n <= hi;
}

bool is_num(const ce::Json &v, double lo, double hi)
{
    return v.type == ce::Json::Num && std::isfinite(v.n) && v.n >= lo && v.n <= hi;
}

bool at_zero(const SessionPlan &p, double at)
{
    return p.unit == kLap ? at <= 1.0 : at <= 0.0;
}

int plan_states(const SessionPlan &p)
{
    return static_cast<int>(p.pts.size()) + (p.pts.empty() || at_zero(p, p.pts[0].at) ? 0 : 1);
}

bool parse_session(const ce::Json &v, const char *key, SessionPlan &p, std::string &err)
{
    auto fail = [&](const std::string &what) {
        err = std::string("sessions.") + key + ": " + what;
        return false;
    };
    if (v.type != ce::Json::Obj)
        return fail("must be an object or null");
    const ce::Json &tl = v["timeline"];
    if (tl.type != ce::Json::Arr || tl.a.empty() || tl.a.size() > kMaxPlanStates)
        return fail("timeline must be an array of 1..6 points");
    const ce::Json &laps = v["laps"];
    if (laps.type != ce::Json::Null) {
        if (!is_int(laps, 1, 200))
            return fail("laps must be an integer 1..200; a lap-based change needs the race laps");
        p.laps = static_cast<int>(laps.n);
    }
    const ce::Json &mins = v["minutes"];
    if (mins.type != ce::Json::Null) {
        if (!is_int(mins, 1, 240))
            return fail("minutes must be an integer 1..240");
        p.minutes = static_cast<int>(mins.n);
    }
    const ce::Json &start = v["start"];
    if (start.type != ce::Json::Null) {
        p.start = start.type == ce::Json::Str ? state_of(start.s) : -1;
        if (p.start < 0)
            return fail("start must be a state name");
    }
    const ce::Json &air = v["air_temp"];
    if (air.type != ce::Json::Null) {
        if (!is_num(air, -10, 50))
            return fail("air_temp must be -10..50");
        p.has_air = true;
        p.air = air.n;
    }
    const ce::Json &trk = v["track_temp"];
    if (trk.type != ce::Json::Null) {
        if (!is_num(trk, -10, 70))
            return fail("track_temp must be -10..70");
        p.has_track = true;
        p.track = trk.n;
    }
    p.unit = -1;
    for (size_t i = 0; i < tl.a.size(); ++i) {
        const ce::Json &e = tl.a[i];
        std::string at = "timeline[" + std::to_string(i) + "]";
        if (e.type != ce::Json::Obj)
            return fail(at + " must be an object");
        int unit = -1, found = 0;
        for (int u = 0; u < 3; ++u)
            if (e[kUnitKey[u]].type != ce::Json::Null) {
                unit = u;
                ++found;
            }
        if (found != 1)
            return fail(at + " needs exactly one of at_fraction, at_minute, at_lap");
        if (p.unit >= 0 && unit != p.unit)
            return fail(at + " mixes position units; use one unit per session");
        p.unit = unit;
        Point pt;
        const ce::Json &pos = e[kUnitKey[unit]];
        if (unit == kFraction && !is_num(pos, 0, 1))
            return fail(at + ".at_fraction must be 0..1");
        if (unit == kMinute && !is_num(pos, 0, 240))
            return fail(at + ".at_minute must be 0..240");
        if (unit == kLap && !is_int(pos, 1, 200))
            return fail(at + ".at_lap must be an integer 1..200");
        pt.at = pos.n;
        if (unit == kLap && timed_session(key))
            return fail(at + ".at_lap needs a race; this session is timed, use at_minute or at_fraction");
        if (unit == kLap && !p.laps)
            return fail(at + ".at_lap needs laps on the session (laps is missing or 0)");
        if (unit == kLap && pt.at > p.laps)
            return fail(at + ".at_lap needs laps on the session and at_lap <= laps");
        if (unit == kMinute && p.minutes && pt.at > p.minutes)
            return fail(at + ".at_minute is past the session's minutes");
        const ce::Json &st = e["state"];
        pt.state = st.type == ce::Json::Str ? state_of(st.s) : -1;
        if (pt.state < 0)
            return fail(at + ".state must be one of clear, light_cloud, overcast, light_rain, heavy_rain, storm");
        const ce::Json &in = e["intensity"];
        if (in.type != ce::Json::Null) {
            if (!is_num(in, 0, 1))
                return fail(at + ".intensity must be 0..1");
            pt.intensity = in.n;
        }
        const ce::Json &rp = e["ramp_s"];
        if (rp.type != ce::Json::Null) {
            if (!is_num(rp, 0, 1800))
                return fail(at + ".ramp_s must be 0..1800");
            pt.ramp = rp.n;
        }
        if (!p.pts.empty() && pt.at <= p.pts.back().at)
            return fail(at + " must come after the previous point");
        p.pts.push_back(pt);
    }
    if (plan_states(p) > kMaxPlanStates)
        return fail("start plus the timeline is more than 6 states");
    return true;
}

Build make_build(const SessionPlan &p, float len, int game_first)
{
    Build b;
    if (!std::isfinite(len) || len < 60.0f || len > 36000.0f)
        len = 3600.0f;
    if (p.pts.empty()) {
        b.type[0] = static_cast<uint8_t>(p.start >= 0 ? p.start : (game_first >= 0 && game_first < 6) ? game_first : 0);
        b.n = 1;
        b.hold[0] = len;
        return b;
    }
    bool zero = at_zero(p, p.pts[0].at);
    size_t first_change = zero ? 1 : 0;
    b.type[0] = static_cast<uint8_t>(zero ? p.pts[0].state : p.start >= 0 ? p.start
                                                          : (game_first >= 0 && game_first < 6) ? game_first
                                                                                                : 0);
    b.inten[0] = zero && p.pts[0].intensity >= 0 ? static_cast<float>(p.pts[0].intensity) : -1.0f;
    b.n = 1;
    float ramp_default = std::fmin(std::fmax(0.02f * len, 30.0f), 300.0f);
    float prev_end = 0;
    for (size_t k = first_change; k < p.pts.size() && b.n < kMaxPlanStates; ++k) {
        float t = static_cast<float>(fraction(p, p.pts[k].at, len) * len);
        float floor_t = prev_end + kMinHoldS;
        if (t < floor_t)
            t = floor_t;
        if (t + kMinRampS + kMinHoldS > len) {
            b.dropped = static_cast<uint32_t>(p.pts.size() - k);
            break;
        }
        float next = k + 1 < p.pts.size() ? static_cast<float>(fraction(p, p.pts[k + 1].at, len) * len) : len;
        float ramp = p.pts[k].ramp >= 0 ? static_cast<float>(p.pts[k].ramp) : ramp_default;
        ramp = std::fmin(ramp, std::fmax(0.0f, next - t));
        ramp = std::fmin(ramp, std::fmax(0.0f, len - t - kMinHoldS));
        ramp = std::fmax(ramp, kMinRampS);
        b.hold[b.n - 1] = t - prev_end;
        b.trans[b.n - 1] = ramp;
        prev_end = t + ramp;
        b.type[b.n] = static_cast<uint8_t>(p.pts[k].state);
        b.inten[b.n] = p.pts[k].intensity >= 0 ? static_cast<float>(p.pts[k].intensity) : -1.0f;
        ++b.n;
    }
    b.hold[b.n - 1] = std::fmax(len - prev_end, 1.0f);
    b.trans[b.n - 1] = 0;
    return b;
}

std::string validate_build(const Build &b, float len)
{
    char buf[160];
    if (!std::isfinite(len) || len <= 0)
        return "session length is not a positive number";
    if (b.n < 1 || b.n > kMaxPlanStates) {
        std::snprintf(buf, sizeof buf, "%u states (1..%d allowed)", b.n, kMaxPlanStates);
        return buf;
    }
    double total = 0;
    for (uint32_t i = 0; i < b.n; ++i) {
        bool last = i + 1 == b.n;
        if (b.type[i] >= 6) {
            std::snprintf(buf, sizeof buf, "state %u has type %u", i, b.type[i]);
            return buf;
        }
        if (!std::isfinite(b.hold[i]) || b.hold[i] < kMinHoldS) {
            std::snprintf(buf, sizeof buf, "state %u hold %g s is not a finite time of at least %g s", i, b.hold[i],
                          kMinHoldS);
            return buf;
        }
        if (!std::isfinite(b.trans[i]) || (last ? b.trans[i] != 0 : b.trans[i] < kMinRampS)) {
            std::snprintf(buf, sizeof buf, "state %u transition %g s is not finite or is shorter than %g s", i,
                          b.trans[i], kMinRampS);
            return buf;
        }
        if (!std::isfinite(b.inten[i]) || b.inten[i] > 1.0f) {
            std::snprintf(buf, sizeof buf, "state %u intensity %g is not a number up to 1", i, b.inten[i]);
            return buf;
        }
        total += static_cast<double>(b.hold[i]) + b.trans[i];
    }
    if (!(total <= static_cast<double>(len) * 1.0001 + 0.01)) {
        std::snprintf(buf, sizeof buf, "states add up to %g s, past the %g s session", total, len);
        return buf;
    }
    return "";
}

std::vector<std::string> clamp_build(Build &b, const TypeRange (&ranges)[6])
{
    std::vector<std::string> out;
    for (uint32_t i = 0; i < b.n; ++i) {
        float v = b.inten[i];
        if (v < 0 || b.type[i] >= 6)
            continue;
        const TypeRange &r = ranges[b.type[i]];
        bool usable = std::isfinite(r.min) && std::isfinite(r.max) && r.min >= 0 && r.min <= r.max && r.max <= 1;
        float to = !usable ? -1.0f : v < r.min ? r.min : v > r.max ? r.max : v;
        if (to == v)
            continue;
        b.inten[i] = to;
        char buf[128];
        if (usable)
            std::snprintf(buf, sizeof buf, "state %u %s intensity %.2f -> %.2f (game range %.2f..%.2f)", i,
                          kStates[b.type[i]], v, to, r.min, r.max);
        else
            std::snprintf(buf, sizeof buf, "state %u %s intensity %.2f -> the game's draw (game range %g..%g unusable)",
                          i, kStates[b.type[i]], v, r.min, r.max);
        out.push_back(buf);
    }
    return out;
}

} // namespace ce::weather
