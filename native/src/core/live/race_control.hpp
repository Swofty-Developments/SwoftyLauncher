#pragma once
#include <swofty/systems/race_control.hpp>

#include <cstdint>
#include <cstring>
#include <string_view>
#include <vector>

#include "core/live/live_snapshot.hpp"
#include "core/live/live_udp.hpp"

// Race control from the live snapshot (std lib race_control, swofty/systems/race_control.hpp): the tracker keeps the
// last state it read and turns each new snapshot into the moments that changed, in the order the core fires them:
// red flag, safety car, flag, then each car's result status and sector. A session starts again (and its first read
// fires nothing) when the snapshot is not valid, or its session type, car set generation, game mode or track
// changes. A value whose have bit is clear in a snapshot is forgotten and set again, without a moment, when it comes
// back. Pure: the frame step (race_control.cpp) feeds it and fires what it returns.

namespace swofty_core::live::race_control {

namespace rc = swofty::race_control;

// The points race control fires. They fire only while the live state is on, so the hooks table reports them live
// only then (hooks.cpp h_live).
inline bool fires(std::string_view point)
{
    return point == rc::events::Flag::point || point == rc::events::SafetyCar::point || point == rc::events::RedFlag::point ||
           point == rc::events::Sector::point || point == rc::events::CarStatus::point || point == rc::events::Penalty::point ||
           point == rc::events::PenaltyServed::point || point == rc::events::Drs::point ||
           point == rc::events::StartLights::point || point == rc::events::SafetyCarPhase::point ||
           point == rc::events::FastestLap::point || point == rc::events::Classification::point;
}

struct Moments {
    std::vector<rc::events::RedFlag> red_flags;
    std::vector<rc::events::SafetyCar> safety_cars;
    std::vector<rc::events::Flag> flags;
    std::vector<rc::events::CarStatus> statuses;
    std::vector<rc::events::Sector> sectors;
    std::vector<rc::events::Drs> drs;
    std::vector<rc::events::StartLights> lights;
    std::vector<rc::events::SafetyCarPhase> phases;
    std::vector<rc::events::FastestLap> fastest;

    bool empty() const
    {
        return red_flags.empty() && safety_cars.empty() && flags.empty() && statuses.empty() && sectors.empty() && drs.empty() &&
               lights.empty() && phases.empty() && fastest.empty();
    }
};

template <class E> E payload()
{
    E e{};
    e.size = uint32_t(sizeof(E));
    e.payload_version = E::version;
    return e;
}

inline int32_t leader_lap(const Snapshot &s)
{
    if (!(s.ses.have & SWOFTY_LIVE_SHAVE_ORDER) || !s.ses.classified)
        return 0;
    uint32_t id = s.ses.order[0];
    if (id >= SWOFTY_LIVE_MAX_CARS || !s.present[id] || !(s.cars[id].have & SWOFTY_LIVE_HAVE_PROGRESS))
        return 0;
    return s.cars[id].lap;
}

// The session a snapshot belongs to: a new key starts the trackers' session again.
struct SessionKey {
    uint32_t type = 0, generation = 0;
    uint8_t game_mode = 0;
    char track[sizeof(SwoftyLiveSession::track_name)] = {};

    bool operator==(const SessionKey &o) const
    {
        return type == o.type && generation == o.generation && game_mode == o.game_mode &&
               !std::memcmp(track, o.track, sizeof track);
    }
};

inline SessionKey session_key(const Snapshot &s)
{
    SessionKey k;
    k.type = (s.ses.have & SWOFTY_LIVE_SHAVE_TYPE) ? s.ses.type : 0;
    k.generation = s.ses.generation;
    k.game_mode = (s.ses.have & SWOFTY_LIVE_SHAVE_GAME_MODE) ? s.ses.game_mode : 0;
    if (s.ses.have & SWOFTY_LIVE_SHAVE_TRACK_NAME)
        std::memcpy(k.track, s.ses.track_name, sizeof k.track);
    return k;
}

class Tracker {
public:
    void reset()
    {
        primed_ = false;
        flag_known_ = sc_known_ = red_known_ = false;
        broadcast_ = Broadcast{};
        for (CarSeen &c : cars_)
            c = CarSeen{};
    }

    bool primed() const { return primed_; }
    uint64_t sessions() const { return sessions_; }

    Moments step(const Snapshot *s)
    {
        Moments out;
        if (!s || !s->valid) {
            reset();
            return out;
        }
        SessionKey k = session_key(*s);
        if (!primed_ || !(k == key_)) {
            reset();
            key_ = k;
            primed_ = true;
            sessions_++;
        }
        rc::State st = rc::read(s->ses, leader_lap(*s));
        session_moments(st, out);
        broadcast_moments(*s, st, out);
        car_moments(*s, out);
        return out;
    }

private:
    struct CarSeen {
        bool status_known = false, sector_known = false;
        rc::Status status = rc::Status::invalid;
        uint8_t sector = 0;
    };

    void session_moments(const rc::State &st, Moments &out)
    {
        if (st.red_flag == rc::RedFlag::unknown)
            red_known_ = false;
        else if (!red_known_ || st.red_flag != red_) {
            if (red_known_) {
                auto e = payload<rc::events::RedFlag>();
                e.state = st.red_flag;
                e.previous = red_;
                e.lap = st.lap;
                out.red_flags.push_back(e);
            }
            red_known_ = true;
            red_ = st.red_flag;
        }

        if (st.safety_car == rc::Deployment::unknown)
            sc_known_ = false;
        else if (!sc_known_ || st.safety_car != sc_) {
            if (sc_known_) {
                auto e = payload<rc::events::SafetyCar>();
                e.deployment = st.safety_car;
                e.previous = sc_;
                e.lap = st.lap;
                out.safety_cars.push_back(e);
            }
            sc_known_ = true;
            sc_ = st.safety_car;
        }

        if (st.flag == rc::Flag::unknown)
            flag_known_ = false;
        else if (!flag_known_ || st.flag != flag_ || st.yellow_sectors != yellow_) {
            if (flag_known_) {
                auto e = payload<rc::events::Flag>();
                e.flag = st.flag;
                e.previous = flag_;
                e.yellow_sectors = st.yellow_sectors;
                e.previous_yellow_sectors = yellow_;
                e.lap = st.lap;
                out.flags.push_back(e);
            }
            flag_known_ = true;
            flag_ = st.flag;
            yellow_ = st.yellow_sectors;
        }
    }

    // The values the UDP source fills from race control's own messages: absent is a value of its own, so the first
    // DRS, start light, safety car phase and fastest lap of a session fire once the session's first read is done.
    struct Broadcast {
        bool seen = false;
        uint8_t drs = 0xff, lights = 0xff, phase = 0xff;
        uint32_t fastest_car = SWOFTY_LIVE_NO_CAR, fastest_ms = 0;
    };

    void broadcast_moments(const Snapshot &s, const rc::State &st, Moments &out)
    {
        const SwoftyLiveSession &ses = s.ses;
        Broadcast now;
        now.seen = true;
        if (ses.have & SWOFTY_LIVE_SHAVE_DRS)
            now.drs = ses.drs_enabled;
        if (ses.have & SWOFTY_LIVE_SHAVE_START_LIGHTS)
            now.lights = ses.start_lights;
        if (ses.have & SWOFTY_LIVE_SHAVE_SAFETY_CAR_PHASE)
            now.phase = ses.safety_car_phase;
        if (ses.have & SWOFTY_LIVE_SHAVE_FASTEST_LAP)
            now.fastest_car = ses.fastest_lap_vehicle, now.fastest_ms = ses.fastest_lap_ms;
        Broadcast was = broadcast_;
        broadcast_ = now;
        if (!was.seen)
            return;
        if (now.drs != 0xff && now.drs != was.drs) {
            auto e = payload<rc::events::Drs>();
            e.enabled = now.drs;
            e.reason = ses.drs_reason;
            e.lap = st.lap;
            out.drs.push_back(e);
        }
        if (now.lights != 0xff && now.lights != was.lights && (now.lights || was.lights != 0xff)) {
            auto e = payload<rc::events::StartLights>();
            e.lights = now.lights;
            e.out = now.lights == 0;
            e.lap = st.lap;
            out.lights.push_back(e);
        }
        if (now.phase != 0xff && now.phase != was.phase) {
            auto e = payload<rc::events::SafetyCarPhase>();
            e.deployment = st.safety_car;
            e.phase = now.phase;
            e.previous = was.phase;
            e.lap = st.lap;
            out.phases.push_back(e);
        }
        if (now.fastest_ms && (now.fastest_car != was.fastest_car || now.fastest_ms != was.fastest_ms)) {
            auto e = payload<rc::events::FastestLap>();
            e.car = now.fastest_car;
            e.lap_ms = now.fastest_ms;
            e.previous_car = was.fastest_car;
            e.previous_ms = was.fastest_ms;
            uint32_t id = now.fastest_car;
            e.lap =
                id < SWOFTY_LIVE_MAX_CARS && s.present[id] && (s.cars[id].have & SWOFTY_LIVE_HAVE_PROGRESS) ? s.cars[id].lap : 0;
            out.fastest.push_back(e);
        }
    }

    void car_moments(const Snapshot &s, Moments &out)
    {
        for (uint32_t id = 0; id < SWOFTY_LIVE_MAX_CARS; id++) {
            CarSeen &seen = cars_[id];
            if (!s.present[id]) {
                seen = CarSeen{};
                continue;
            }
            const SwoftyLiveCar &c = s.cars[id];
            int32_t lap = (c.have & SWOFTY_LIVE_HAVE_PROGRESS) ? c.lap : 0;
            if (auto st = rc::status(c)) {
                if (seen.status_known && *st != seen.status) {
                    auto e = payload<rc::events::CarStatus>();
                    e.car = id;
                    e.status = *st;
                    e.previous = seen.status;
                    e.lap = lap;
                    out.statuses.push_back(e);
                }
                seen.status_known = true;
                seen.status = *st;
            } else
                seen.status_known = false;
            if (auto sec = rc::sector(c)) {
                if (seen.sector_known && *sec != seen.sector) {
                    auto e = payload<rc::events::Sector>();
                    e.car = id;
                    e.sector = *sec;
                    e.previous = seen.sector;
                    e.lap = lap;
                    out.sectors.push_back(e);
                }
                seen.sector_known = true;
                seen.sector = *sec;
            } else
                seen.sector_known = false;
        }
    }

    bool primed_ = false;
    uint64_t sessions_ = 0;
    SessionKey key_;
    bool flag_known_ = false, sc_known_ = false, red_known_ = false;
    rc::Flag flag_ = rc::Flag::unknown;
    uint8_t yellow_ = 0;
    rc::Deployment sc_ = rc::Deployment::unknown;
    rc::RedFlag red_ = rc::RedFlag::unknown;
    Broadcast broadcast_;
    CarSeen cars_[SWOFTY_LIVE_MAX_CARS];
};

}

namespace runtime {

// Once per live tick, on the flow thread: reads the live state's published snapshot and fires what changed.
void race_control_tick(const swofty_core::live::Snapshot &published);

// The penalties and served penalties the live state's UDP source decoded since the last call (live_state.cpp).
swofty_core::live::udp::Moments live_udp_take();

}
