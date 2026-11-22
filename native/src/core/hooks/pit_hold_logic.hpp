#pragma once

#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstring>

// The extra box hold of the session operations (pit_hold.cpp). An addon arms a car's next stop with a number of
// seconds; the core fixes that number when the car enters the pit lane and adds it to the game's own box hold
// (Game_PitLane_UpdateMovement keeps the car in the box until its box clock passes the crew's job time plus the hold
// seconds, which are the stop-go length or 0). The crew waits through those seconds before it works:
//   stretch  the game's 5 s stop-go crew clip (StartJobDoStandardStop...5SP) plays with only its wait timed to the
//            hold, the work after the clip's Start event at normal speed
//   freeze   the plain stop clip stands still for the hold seconds from its first frame, then plays
//   none     the crew works at once and the car waits after it
// Stretch falls back to freeze for a stop whose SP clip does not exist. A stop counts as served when the car leaves
// InPitBox for InPitBox_CrewCompleted, InPitBoxPerformedRelease or LeavingPitBox, which the game sets only once the
// box clock covers the hold. Every member is an atomic: the pit lane update, the crew jobs and the status changes run
// on different game threads.

namespace swofty_core::hooks::pit_hold {

inline constexpr uint32_t CARS = 27;
inline constexpr float MAX_SECONDS = 600.0f;
inline constexpr float STRETCH_BASE = 5.0f;
inline constexpr float WAIT_SLACK = 2.0f;

enum Status : uint32_t {
    ON_TRACK = 1,
    ENTERING_PIT_LANE = 2,
    ENTERING_PIT_BOX = 3,
    IN_PIT_BOX = 4,
    CREW_COMPLETED = 5,
    PERFORMED_RELEASE = 6,
    LEAVING_PIT_BOX = 7,
};

enum Crew : uint32_t { CREW_NONE = 0, CREW_STRETCH = 1, CREW_FREEZE = 2 };

inline bool in_lane(uint32_t s) { return s >= ENTERING_PIT_LANE && s <= LEAVING_PIT_BOX; }

inline bool served_exit(uint32_t s) { return s == CREW_COMPLETED || s == PERFORMED_RELEASE || s == LEAVING_PIT_BOX; }

inline float clean(float seconds)
{
    if (!std::isfinite(seconds) || seconds <= 0.0f)
        return 0.0f;
    return seconds > MAX_SECONDS ? MAX_SECONDS : seconds;
}

inline bool crew_from_name(const char *s, Crew &out)
{
    if (!s || !*s)
        return false;
    if (!std::strcmp(s, "stretch"))
        out = CREW_STRETCH;
    else if (!std::strcmp(s, "freeze"))
        out = CREW_FREEZE;
    else if (!std::strcmp(s, "none"))
        out = CREW_NONE;
    else
        return false;
    return true;
}

inline const char *crew_name(uint32_t c) { return c == CREW_STRETCH ? "stretch" : c == CREW_FREEZE ? "freeze" : "none"; }

struct Car {
    std::atomic<float> armed{0};
    std::atomic<float> stop{0};
    std::atomic<float> served{0};
    std::atomic<uint32_t> serves{0};
    std::atomic<uint32_t> status{0};
    std::atomic<uint32_t> crew{CREW_NONE};
    std::atomic<uint32_t> waiting{0};
    std::atomic<float> waited{0};
    std::atomic<float> base{STRETCH_BASE};
    std::atomic<uint32_t> last_crew{CREW_NONE};

    void reset()
    {
        armed = 0;
        stop = 0;
        served = 0;
        serves = 0;
        status = 0;
        crew = CREW_NONE;
        waiting = 0;
        waited = 0;
        base = STRETCH_BASE;
        last_crew = CREW_NONE;
    }
};

struct Snapshot {
    float armed, stop, served;
    uint32_t serves, status, crew;
};

class Table {
public:
    Car &car(uint32_t c) { return cars_[c < CARS ? c : 0]; }

    void set_crew(Crew c) { mode_ = c; }
    Crew crew_mode() const { return Crew(mode_.load()); }

    bool arm(uint32_t c, float seconds)
    {
        if (c >= CARS)
            return false;
        cars_[c].armed = clean(seconds);
        return true;
    }

    // A status change of car c, from the game's Game_Pitlane_SetVehicleStatus. Returns the seconds a stop just served
    // (0 for any other change).
    float on_status(uint32_t c, uint32_t before, uint32_t after)
    {
        if (c >= CARS)
            return 0;
        Car &k = cars_[c];
        k.status = after;
        if (!in_lane(before) && in_lane(after)) {
            float s = k.armed.load();
            k.stop = s;
            k.crew = s > 0 ? mode_.load() : uint32_t(CREW_NONE);
            k.waiting = 0;
            k.waited = 0;
            return 0;
        }
        if (before == IN_PIT_BOX && served_exit(after) && k.stop.load() > 0) {
            float s = k.stop.exchange(0);
            k.served = s;
            k.serves.fetch_add(1);
            k.armed = 0;
            k.last_crew = k.crew.load();
            k.waiting = 0;
            return s;
        }
        if (!in_lane(after)) {
            k.stop = 0;
            k.waiting = 0;
        }
        return 0;
    }

    // The seconds the box hold site adds for car c this frame.
    float box_extra(uint32_t c) const
    {
        if (c >= CARS)
            return 0;
        const Car &k = cars_[c];
        return k.status.load(std::memory_order_relaxed) == IN_PIT_BOX ? k.stop.load(std::memory_order_relaxed) : 0;
    }

    // Whether the crew of car c plays the stop-go clip at this stop (the 142f29600 answer at the crew call sites).
    bool crew_serves(uint32_t c) const
    {
        if (c >= CARS)
            return false;
        const Car &k = cars_[c];
        return k.crew.load(std::memory_order_relaxed) == CREW_STRETCH && k.stop.load(std::memory_order_relaxed) > 0 &&
               in_lane(k.status.load(std::memory_order_relaxed));
    }

    // The crew of car c starts its box clip: a stop-go clip of base seconds (sp) or a plain one; sp_missing when the
    // stop-go clip did not load and the plain one plays instead. Returns the crew mode the stop runs.
    uint32_t clip_started(uint32_t c, bool sp, bool sp_missing, float base = STRETCH_BASE)
    {
        if (c >= CARS)
            return CREW_NONE;
        Car &k = cars_[c];
        if (k.stop.load() <= 0 || k.waiting.load())
            return k.waiting.load() ? k.crew.load() : uint32_t(CREW_NONE);
        uint32_t m = k.crew.load();
        if (m == CREW_STRETCH && (!sp || sp_missing))
            k.crew = m = CREW_FREEZE;
        if (m == CREW_NONE)
            return m;
        k.base = base > 0 && std::isfinite(base) ? base : STRETCH_BASE;
        k.waited = 0;
        k.waiting = 1;
        return m;
    }

    // The crew job's frame step for car c: dt is the game's step (game seconds times the crew rate), rate that rate.
    // Returns the step the job takes instead.
    float crew_step(uint32_t c, float dt, float rate)
    {
        if (c >= CARS)
            return dt;
        Car &k = cars_[c];
        if (!k.waiting.load(std::memory_order_relaxed))
            return dt;
        float s = k.stop.load(std::memory_order_relaxed);
        if (s <= 0 || !(rate > 0) || !std::isfinite(dt)) {
            k.waiting = 0;
            return dt;
        }
        float real = dt / rate;
        float waited = k.waited.load(std::memory_order_relaxed) + real;
        k.waited.store(waited, std::memory_order_relaxed);
        uint32_t m = k.crew.load(std::memory_order_relaxed);
        if (m == CREW_FREEZE) {
            if (waited < s)
                return 0.0f;
            k.waiting = 0;
            return (waited - s) * rate;
        }
        if (waited > s + WAIT_SLACK) {
            k.waiting = 0;
            return dt;
        }
        return real * k.base.load(std::memory_order_relaxed) / s;
    }

    // The stop-go clip's Start event for car c: the crew starts working.
    bool crew_start(uint32_t c)
    {
        if (c >= CARS)
            return false;
        Car &k = cars_[c];
        if (k.crew.load() != CREW_STRETCH || !k.waiting.load())
            return false;
        k.waiting = 0;
        return true;
    }

    Snapshot snapshot(uint32_t c) const
    {
        const Car &k = cars_[c < CARS ? c : 0];
        return {k.armed.load(), k.stop.load(), k.served.load(), k.serves.load(), k.status.load(), k.last_crew.load()};
    }

    void reset_all()
    {
        for (auto &k : cars_)
            k.reset();
    }

private:
    Car cars_[CARS];
    std::atomic<uint32_t> mode_{CREW_STRETCH};
};

}
