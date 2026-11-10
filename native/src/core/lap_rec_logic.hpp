#pragma once

#include <cmath>
#include <cstdint>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

namespace runtime::lap_rec {

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
    }
    return "";
}

struct Config {
    size_t max_samples = 60 * 60 * 10;
    size_t min_samples = 30;
    double min_lap_s = 20.0;
    double min_path_m = 400.0;
    double teleport_floor_m = 30.0;
    double max_gap_s = 5.0;
    double reset_from_ms = 30.0;
    double reset_to_ms = 1.0;
};

struct Frame {
    double now_s = 0;
    double pos[3] = {0, 0, 0};
    double speed_ms = 0;
    bool player = true;
};

struct Lap {
    std::vector<uint8_t> samples;
    uint32_t count = 0;
    double started_s = 0, ended_s = 0, path_m = 0;
    uint32_t index = 0;
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
    size_t samples() const { return count_; }
    double path_m() const { return path_; }
    double started_s() const { return started_; }
    uint32_t laps_written() const { return index_; }

    void begin_session()
    {
        clear();
        state_ = State::outlap;
        index_ = 0;
    }

    Outcome end_session()
    {
        Outcome o = discarding(Discard::left_session);
        state_ = State::idle;
        return o;
    }

    double elapsed(double now_s) const { return now_s - started_; }

    // The car moved without driving there, or stopped dead in a tick: the lap in progress is thrown away and the
    // next line crossing starts a fresh one.
    Outcome step(const Frame &f, const uint8_t *sample, size_t stride)
    {
        Outcome none;
        if (!f.player || state_ == State::idle)
            return none;
        Discard bad = Discard::none;
        if (have_last_) {
            double dt = f.now_s - last_.now_s;
            double dx = f.pos[0] - last_.pos[0], dy = f.pos[1] - last_.pos[1], dz = f.pos[2] - last_.pos[2];
            double step = std::sqrt(dx * dx + dy * dy + dz * dz);
            double allowed = cfg_.teleport_floor_m + 1.5 * std::fmax(f.speed_ms, last_.speed_ms) * std::fmax(dt, 0.0);
            if (dt < 0 || dt > cfg_.max_gap_s || step > allowed)
                bad = Discard::teleport;
            else if (last_.speed_ms > cfg_.reset_from_ms && f.speed_ms < cfg_.reset_to_ms)
                bad = Discard::reset;
            else if (state_ == State::timing)
                path_ += step;
        }
        last_ = f;
        have_last_ = true;
        if (bad != Discard::none)
            return discarding(bad);
        if (state_ != State::timing)
            return none;
        if (count_ >= cfg_.max_samples)
            return discarding(Discard::capped);
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
    uint32_t index_ = 0;
    Frame last_;
    bool have_last_ = false;
};

}
