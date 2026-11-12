#pragma once

#include "lap_file_logic.hpp"

#include <swofty/json.hpp>

#include <cmath>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

namespace runtime::lap_rec {

// The session's board needs the game's current-track getter, which costs 100 ms or more a call. While the track is not known
// the tick asks again once a second, at most BOARD_TRIES times a session; a lap's timing start asks once more whatever the count.
constexpr int BOARD_TRIES = 10;

inline bool board_read_due(bool forced, int tries)
{
    return forced || tries < BOARD_TRIES;
}

// Three basis vectors of a body, one per row of c, as read from memory.
inline bool unit_axes(const float m[3][3])
{
    for (int c = 0; c < 3; c++) {
        float len = m[c][0] * m[c][0] + m[c][1] * m[c][1] + m[c][2] * m[c][2];
        if (!(len > 0.95f && len < 1.05f))
            return false;
    }
    for (int a = 0; a < 3; a++)
        for (int b = a + 1; b < 3; b++)
            if (std::fabs(m[a][0] * m[b][0] + m[a][1] * m[b][1] + m[a][2] * m[b][2]) > 0.05f)
                return false;
    return true;
}

// The matrix whose columns are the three basis vectors, as a unit quaternion (x, y, z, w).
inline void quaternion_of(const float c[3][3], float q[4])
{
    float m00 = c[0][0], m10 = c[0][1], m20 = c[0][2], m01 = c[1][0], m11 = c[1][1], m21 = c[1][2], m02 = c[2][0],
          m12 = c[2][1], m22 = c[2][2];
    float tr = m00 + m11 + m22;
    if (tr > 0) {
        float s = std::sqrt(tr + 1.0f) * 2;
        q[3] = 0.25f * s;
        q[0] = (m21 - m12) / s;
        q[1] = (m02 - m20) / s;
        q[2] = (m10 - m01) / s;
    } else if (m00 > m11 && m00 > m22) {
        float s = std::sqrt(1.0f + m00 - m11 - m22) * 2;
        q[3] = (m21 - m12) / s;
        q[0] = 0.25f * s;
        q[1] = (m01 + m10) / s;
        q[2] = (m02 + m20) / s;
    } else if (m11 > m22) {
        float s = std::sqrt(1.0f + m11 - m00 - m22) * 2;
        q[3] = (m02 - m20) / s;
        q[0] = (m01 + m10) / s;
        q[1] = 0.25f * s;
        q[2] = (m12 + m21) / s;
    } else {
        float s = std::sqrt(1.0f + m22 - m00 - m11) * 2;
        q[3] = (m10 - m01) / s;
        q[0] = (m02 + m20) / s;
        q[1] = (m12 + m21) / s;
        q[2] = 0.25f * s;
    }
}

enum class State : uint8_t { idle, outlap, timing };

enum class Discard : uint8_t {
    none,
    left_session,
    teleport,
    reset,
    too_short,
    too_few_samples,
    capped,
    backwards,
    interrupted,
    stationary,
};

inline const char *discard_name(Discard d)
{
    switch (d) {
    case Discard::none: return "none";
    case Discard::left_session: return "the session ended";
    case Discard::teleport: return "the car was moved";
    case Discard::reset: return "the car was reset";
    case Discard::too_short: return "the lap was too short";
    case Discard::too_few_samples: return "the lap has too few samples";
    case Discard::capped: return "the lap is longer than the sample cap";
    case Discard::backwards: return "the line was crossed backwards";
    case Discard::interrupted: return "a replay, cutscene or spectating took over the car";
    case Discard::stationary: return "the car stood still for too long";
    }
    return "";
}

struct Config {
    size_t max_samples = 60 * 60 * 12;
    double sample_period_s = 1.0 / 60.0;
    size_t min_samples = 30;
    double min_lap_s = 20.0;
    double min_path_m = 400.0;
    double teleport_floor_m = 30.0;
    double max_gap_s = 5.0;
    double reset_from_ms = 30.0;
    double reset_to_ms = 1.0;
    double still_speed_ms = 0.5;
    double max_still_s = 30.0;
    double stall_gap_s = 0.5;
    double stall_move_m = 0.25;
    double stall_min_speed_ms = 5.0;
};

struct Frame {
    double now_s = 0;
    double pos[3] = {0, 0, 0};
    double speed_ms = 0;
    bool player = true;
    bool interrupted = false;
};

struct Lap {
    std::vector<uint8_t> samples;
    uint32_t count = 0;
    double started_s = 0, ended_s = 0, path_m = 0;
    uint32_t index = 0;
    double shift_s = 0;
};

struct Outcome {
    enum Kind : uint8_t { none, started, completed, discarded } kind = none;
    Discard why = Discard::none;
    Lap lap;
};

class Recorder {
public:
    explicit Recorder(Config c = {}) : cfg_(c) {}

    State state() const { return state_; }
    const Config &config() const { return cfg_; }
    size_t samples() const { return count_; }
    double path_m() const { return path_; }
    double started_s() const { return started_; }
    uint32_t laps_written() const { return index_; }

    void begin_session()
    {
        clear();
        state_ = State::outlap;
        index_ = 0;
        shift_ = 0;
        stalls_ = 0;
        still_s_ = 0;
    }

    Outcome end_session()
    {
        Outcome o = discarding(Discard::left_session);
        state_ = State::idle;
        return o;
    }

    double elapsed(double now_s) const { return now_s - shift_ - started_; }
    double shift_s() const { return shift_; }
    uint32_t stalls() const { return stalls_; }

    // A step at now_s would keep its sample: the caller reads the car only then, not on every game tick.
    bool sample_due(double now_s) const
    {
        if (state_ != State::timing || cfg_.sample_period_s <= 0 || count_ == 0)
            return true;
        return now_s - shift_ >= next_sample_s_;
    }

    // The car moved without driving there, or stopped dead in a tick: the lap in progress is thrown away and the
    // next line crossing starts a fresh one. A gap in which the car did not move at all while it was driving is a
    // stall of the game (focus lost, a hitch) and is taken out of the lap's clock instead. The sample passed with the
    // stalled frame was built with the clock before the stall was known, so it is not kept: the car is where the last
    // sample left it, and the next frame's sample follows it on the shifted clock. Before a lap's first movement no
    // gap is a stall: at the line the game runs on while the previous lap is written, and its lap time keeps that
    // time, so the clock must too. A replay, a cutscene or spectating discards the lap, and so does a car that
    // stands still for max_still_s.
    Outcome step(const Frame &in, const uint8_t *sample, size_t stride)
    {
        Outcome none;
        if (!in.player || state_ == State::idle)
            return none;
        Frame f = in;
        f.now_s -= shift_;
        if (in.interrupted) {
            last_ = f;
            have_last_ = true;
            return discarding(Discard::interrupted);
        }
        Discard bad = Discard::none;
        bool stalled = false;
        if (have_last_) {
            double dt = f.now_s - last_.now_s;
            double dx = f.pos[0] - last_.pos[0], dy = f.pos[1] - last_.pos[1], dz = f.pos[2] - last_.pos[2];
            double step = std::sqrt(dx * dx + dy * dy + dz * dz);
            bool moving_lap = state_ != State::timing || path_ >= cfg_.stall_move_m;
            if (dt > cfg_.stall_gap_s && step < cfg_.stall_move_m && last_.speed_ms > cfg_.stall_min_speed_ms && moving_lap) {
                double gap = dt - cfg_.sample_period_s;
                shift_ += gap;
                f.now_s -= gap;
                dt = cfg_.sample_period_s;
                stalls_++;
                stalled = true;
            }
            double allowed = cfg_.teleport_floor_m + 1.5 * std::fmax(f.speed_ms, last_.speed_ms) * std::fmax(dt, 0.0);
            if (f.speed_ms < cfg_.still_speed_ms && dt > 0)
                still_s_ += dt;
            else
                still_s_ = 0;
            if (dt < 0 || dt > cfg_.max_gap_s || step > allowed)
                bad = Discard::teleport;
            else if (state_ == State::timing && still_s_ > cfg_.max_still_s)
                bad = Discard::stationary;
            else if (last_.speed_ms > cfg_.reset_from_ms && f.speed_ms < cfg_.reset_to_ms)
                bad = Discard::reset;
            else if (state_ == State::timing)
                path_ += step;
        }
        last_ = f;
        have_last_ = true;
        if (bad != Discard::none)
            return discarding(bad);
        if (state_ != State::timing || stalled)
            return none;
        if (cfg_.sample_period_s > 0 && count_ > 0 && f.now_s < next_sample_s_)
            return none;
        if (count_ >= cfg_.max_samples)
            return discarding(Discard::capped);
        next_sample_s_ = count_ > 0 && f.now_s - next_sample_s_ < cfg_.sample_period_s ? next_sample_s_ + cfg_.sample_period_s
                                                                                       : f.now_s + cfg_.sample_period_s;
        buf_.insert(buf_.end(), sample, sample + stride);
        count_++;
        stride_ = stride;
        return none;
    }

    // The player's car crossed the line: out-laps start timing, a lap in progress ends and the next one begins.
    Outcome newlap(double now_s, bool player)
    {
        Outcome o;
        if (!player || state_ == State::idle)
            return o;
        now_s -= shift_;
        if (state_ == State::outlap) {
            start(now_s);
            o.kind = Outcome::started;
            return o;
        }
        double len = now_s - started_;
        Discard bad = Discard::none;
        if (len < cfg_.min_lap_s)
            bad = Discard::too_short;
        else if (path_ < cfg_.min_path_m)
            bad = Discard::backwards;
        else if (count_ < cfg_.min_samples)
            bad = Discard::too_few_samples;
        if (bad != Discard::none) {
            o = discarding(bad);
            start(now_s);
            return o;
        }
        o.kind = Outcome::completed;
        o.lap.samples = std::move(buf_);
        o.lap.count = uint32_t(count_);
        o.lap.started_s = started_;
        o.lap.ended_s = now_s;
        o.lap.path_m = path_;
        o.lap.index = ++index_;
        o.lap.shift_s = shift_;
        start(now_s);
        return o;
    }

    Outcome reset(Discard why) { return discarding(why); }

private:
    void clear()
    {
        buf_.clear();
        count_ = 0;
        path_ = 0;
        have_last_ = false;
    }

    void start(double now_s)
    {
        buf_.clear();
        count_ = 0;
        path_ = 0;
        started_ = now_s;
        state_ = State::timing;
    }

    Outcome discarding(Discard why)
    {
        Outcome o;
        bool had = state_ == State::timing;
        clear();
        if (state_ == State::timing)
            state_ = State::outlap;
        if (had) {
            o.kind = Outcome::discarded;
            o.why = why;
        }
        return o;
    }

    Config cfg_;
    State state_ = State::idle;
    std::vector<uint8_t> buf_;
    size_t count_ = 0, stride_ = 0;
    double path_ = 0, started_ = 0;
    uint32_t index_ = 0, stalls_ = 0;
    double shift_ = 0, still_s_ = 0;
    Frame last_;
    bool have_last_ = false;
    double next_sample_s_ = 0;
};

enum class Col : uint8_t {
    t, dist, px, py, pz, vx, vy, vz, throttle, brake, steer, gear, rpm, speed, qx, qy, qz, qw,
    ws_rl, ws_rr, ws_fl, ws_fr, g_lat, g_long, drs,
    surf_fl, surf_fr, surf_rl, surf_rr,
    slip_ang_rl, slip_ang_rr, slip_ang_fl, slip_ang_fr,
    slip_ratio_rl, slip_ratio_rr, slip_ratio_fl, slip_ratio_fr,
    count
};

struct Avail {
    bool pos = false, vel = false, pedals = false, steer = false, gear = false, rpm = false, speed = false,
         orient = false, spin = false, g = false, drs = false, surface = false, slip = false;
};

struct ColumnDef {
    Col col;
    const char *name;
    lap_file::Type type;
    const char *unit;
    bool Avail::*needs;
};

inline constexpr ColumnDef COLUMNS[] = {
    {Col::t, "t", lap_file::Type::f32, "s", nullptr},
    {Col::dist, "dist", lap_file::Type::f32, "m", &Avail::pos},
    {Col::px, "px", lap_file::Type::f32, "m", &Avail::pos},
    {Col::py, "py", lap_file::Type::f32, "m", &Avail::pos},
    {Col::pz, "pz", lap_file::Type::f32, "m", &Avail::pos},
    {Col::vx, "vx", lap_file::Type::f32, "m/s", &Avail::vel},
    {Col::vy, "vy", lap_file::Type::f32, "m/s", &Avail::vel},
    {Col::vz, "vz", lap_file::Type::f32, "m/s", &Avail::vel},
    {Col::throttle, "throttle", lap_file::Type::f32, "", &Avail::pedals},
    {Col::brake, "brake", lap_file::Type::f32, "", &Avail::pedals},
    {Col::steer, "steer", lap_file::Type::f32, "", &Avail::steer},
    {Col::gear, "gear", lap_file::Type::i8, "", &Avail::gear},
    {Col::rpm, "rpm", lap_file::Type::f32, "rpm", &Avail::rpm},
    {Col::speed, "speed", lap_file::Type::f32, "km/h", &Avail::speed},
    {Col::qx, "qx", lap_file::Type::f32, "", &Avail::orient},
    {Col::qy, "qy", lap_file::Type::f32, "", &Avail::orient},
    {Col::qz, "qz", lap_file::Type::f32, "", &Avail::orient},
    {Col::qw, "qw", lap_file::Type::f32, "", &Avail::orient},
    {Col::ws_rl, "ws_rl", lap_file::Type::f32, "rad/s", &Avail::spin},
    {Col::ws_rr, "ws_rr", lap_file::Type::f32, "rad/s", &Avail::spin},
    {Col::ws_fl, "ws_fl", lap_file::Type::f32, "rad/s", &Avail::spin},
    {Col::ws_fr, "ws_fr", lap_file::Type::f32, "rad/s", &Avail::spin},
    {Col::g_lat, "g_lat", lap_file::Type::f32, "", &Avail::g},
    {Col::g_long, "g_long", lap_file::Type::f32, "", &Avail::g},
    {Col::drs, "drs", lap_file::Type::f32, "", &Avail::drs},
    {Col::surf_fl, "surf_fl", lap_file::Type::u8, "", &Avail::surface},
    {Col::surf_fr, "surf_fr", lap_file::Type::u8, "", &Avail::surface},
    {Col::surf_rl, "surf_rl", lap_file::Type::u8, "", &Avail::surface},
    {Col::surf_rr, "surf_rr", lap_file::Type::u8, "", &Avail::surface},
    {Col::slip_ang_rl, "slip_ang_rl", lap_file::Type::f32, "rad", &Avail::slip},
    {Col::slip_ang_rr, "slip_ang_rr", lap_file::Type::f32, "rad", &Avail::slip},
    {Col::slip_ang_fl, "slip_ang_fl", lap_file::Type::f32, "rad", &Avail::slip},
    {Col::slip_ang_fr, "slip_ang_fr", lap_file::Type::f32, "rad", &Avail::slip},
    {Col::slip_ratio_rl, "slip_ratio_rl", lap_file::Type::f32, "", &Avail::slip},
    {Col::slip_ratio_rr, "slip_ratio_rr", lap_file::Type::f32, "", &Avail::slip},
    {Col::slip_ratio_fl, "slip_ratio_fl", lap_file::Type::f32, "", &Avail::slip},
    {Col::slip_ratio_fr, "slip_ratio_fr", lap_file::Type::f32, "", &Avail::slip},
};

// The file's field table for the fields that resolved; layout.columns[i] is ids[i]'s column.
struct Plan {
    lap_file::Layout layout;
    std::vector<Col> ids;
    std::string missing;
};

inline Plan plan_of(const Avail &a)
{
    Plan p;
    for (auto &c : COLUMNS) {
        if (c.needs && !(a.*c.needs)) {
            if (p.missing.find(c.name) == std::string::npos)
                p.missing += (p.missing.empty() ? "" : " ") + std::string(c.name);
            continue;
        }
        p.layout.add(c.name, c.type, c.unit);
        p.ids.push_back(c.col);
    }
    return p;
}

// A lap handed to the writer thread and what came back. The flow thread expects laps in the order it hands them
// over and delivers the completions (the bridge's lap_finished, the laps.recorded event) in that same order, so
// a lap never reports before an earlier one, whichever finishes writing first.
struct Finished {
    uint32_t index = 0;
    std::string path;
    int64_t time_ms = 0;
    int valid = -1;
    bool ok = false;
};

class Outbox {
public:
    void expect(uint32_t index) { order_.push_back(index); }

    void complete(Finished f) { done_.push_back(std::move(f)); }

    // The completions whose every earlier lap has completed too, oldest first.
    std::vector<Finished> ready()
    {
        std::vector<Finished> out;
        while (!order_.empty()) {
            size_t i = 0;
            while (i < done_.size() && done_[i].index != order_.front())
                i++;
            if (i == done_.size())
                break;
            out.push_back(std::move(done_[i]));
            done_.erase(done_.begin() + ptrdiff_t(i));
            order_.erase(order_.begin());
        }
        return out;
    }

    size_t in_flight() const { return order_.size(); }
    size_t completed() const { return done_.size(); }

    void clear()
    {
        order_.clear();
        done_.clear();
    }

private:
    std::vector<uint32_t> order_;
    std::vector<Finished> done_;
};

struct Gate {
    bool on = false;
    std::string nonce;
};

inline Gate gate_of(const std::string &text)
{
    Gate g;
    swofty::json::Value v = swofty::json::parse(text);
    if (v.type() != swofty::json::Type::object)
        return g;
    g.on = v["tt_boards"].as_bool(false);
    g.nonce = std::string(v["tt_nonce"].as_string());
    return g;
}

inline const char *weather_of(bool known, double wetness)
{
    return !known ? "" : wetness >= 0.2 ? "wet" : "dry";
}

enum class WeatherFrom { none, wetness, board };

struct Weather {
    std::string word;
    WeatherFrom from = WeatherFrom::none;
};

// The track wetness when the car read has it; else the D/W suffix of the game's own board key for the session.
inline Weather weather_pick(bool known, double wetness, char board_weather)
{
    if (known)
        return {weather_of(true, wetness), WeatherFrom::wetness};
    if (board_weather == 'W' || board_weather == 'D')
        return {board_weather == 'W' ? "wet" : "dry", WeatherFrom::board};
    return {};
}

inline const char *weather_from_name(WeatherFrom f)
{
    return f == WeatherFrom::wetness ? "the track wetness" : f == WeatherFrom::board ? "the game's session board key" : "nowhere";
}

inline std::vector<std::wstring> lap_dirs(const std::wstring &files_root, const std::wstring &swofty_root)
{
    std::vector<std::wstring> dirs;
    for (const std::wstring *root : {&swofty_root, &files_root})
        if (!root->empty() && (dirs.empty() || dirs.front() != *root + L"laps\\"))
            dirs.push_back(*root + L"laps\\");
    return dirs;
}

inline std::string file_name(int year, int month, int day, int hour, int minute, int second, uint32_t n)
{
    char b[64];
    std::snprintf(b, sizeof b, "%04d%02d%02dT%02d%02d%02dZ-%u.swlap", year, month, day, hour, minute, second, n);
    return b;
}

}
