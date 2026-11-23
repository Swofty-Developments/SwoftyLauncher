#pragma once
#include <swofty/systems/race_control.hpp>
#include <swofty_api.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>
#include <vector>

// The live state's UDP source: the session, car and race control values the game's memory gives the core no read
// for, decoded from the game's own UDP packets (the buffer it hands to sendto, before the stock id rewrite) and merged
// into each live snapshot. A field the memory read fills is never overwritten. Packet layouts are the published F1 24
// and F1 25 UDP specification; offsets are from the packet start, after the 29-byte header (u64 session uid at 7):
//   Session (1), 753 bytes, 2024 and 2025: totalLaps 32, sessionTimeLeft u16 38, sessionDuration u16 40,
//     safetyCarStatus 153, pitStopWindowIdealLap 682, pitStopWindowLatestLap 683, pitStopRejoinPosition 684.
//   Lap Data (2): 2024 22 x 50 bytes, 2025 22 x 57, 2026 24 x 57, from 29. With base 30 (2024) or 32 (the rest):
//     safetyCarDelta f32 base-4, driverStatus base+12, pitLaneTimerActive base+14, pitLaneTimeInLaneInMS u16 base+15, pitStopTimerInMS u16
//     base+17.
//   Event (3), 45 bytes, every format: code at 29, details at 33. PENA penaltyType, infringementType, vehicleIdx,
//     otherVehicleIdx, time, lapNum, placesGained; DTSV vehicleIdx; SGSV vehicleIdx, stopTime f32 (2025); FTLP
//     vehicleIdx, lapTime f32 s; DRSE; DRSD reason (2025); STLG numLights; LGOT; SCAR safetyCarType, eventType.
//   Participants (4): 2024 22 x 60 bytes, 2025 22 x 57, from 30: driverId 1, teamId 3, raceNumber 5; 2025 numColours
//     44 and liveryColours[4] RGB from 45. 2026 24 x 60: driverId u16 at 1 and teamId u16 at 5 only.
//   Car Status (7): 2024 and 2025 22 x 55 bytes, from 29: pitLimiterStatus 4, drsAllowed 22, drsActivationDistance
//     u16 23, vehicleFiaFlags i8 28. 2026 24 x 59: drsAllowed 22 and vehicleFiaFlags 28 only.
//   Final Classification (8): numCars 29, entries from 30, 2024 22 x 45 bytes, 2025 22 x 46: position 0, numLaps 1,
//     gridPosition 2, points 3, numPitStops 4, resultStatus 5, resultReason 6 (2025), then bestLapTimeInMS u32,
//     totalRaceTime f64, penaltiesTime.
//   Session History (11), 1460 bytes, 2024 and 2025: carIdx 29, numLaps 30, bestSector1LapNum 33, 2 at 34, 3 at 35,
//     100 x 14-byte laps from 36: sector n ms u16 at 4 + 3(n-1) and minutes u8 after it.
// Pure: live_state.cpp feeds it packets and snapshots under its own lock.

namespace swofty_core::live::udp {

namespace rc = swofty::race_control;

constexpr size_t HEADER = 29;
constexpr unsigned CARS = 24;
constexpr uint64_t STALE_MS = 5000;
constexpr uint64_t PARTICIPANTS_STALE_MS = 15000;

// The freshness clock: wall milliseconds less the time the game spent paused, when it sends no packets.
class Clock {
public:
    uint64_t step(uint64_t now, bool paused)
    {
        if (last_ && paused && now > last_)
            paused_ += now - last_;
        last_ = now;
        return at(now);
    }
    uint64_t at(uint64_t now) const { return now - paused_; }

private:
    uint64_t last_ = 0, paused_ = 0;
};

inline uint16_t u16(const uint8_t *p) { return uint16_t(p[0] | p[1] << 8); }

inline uint32_t u32(const uint8_t *p)
{
    return uint32_t(p[0]) | uint32_t(p[1]) << 8 | uint32_t(p[2]) << 16 | uint32_t(p[3]) << 24;
}

inline uint64_t u64(const uint8_t *p) { return uint64_t(u32(p)) | uint64_t(u32(p + 4)) << 32; }

inline float f32(const uint8_t *p)
{
    uint32_t v = u32(p);
    float f;
    std::memcpy(&f, &v, sizeof f);
    return f;
}

struct Layout {
    int format, id;
    size_t size, stride, cars, first;
};

inline const Layout *layout(const uint8_t *p, size_t len)
{
    static const Layout table[] = {
        {2024, 1, 753, 0, 0, 0},
        {2025, 1, 753, 0, 0, 0},
        {2024, 2, 1131, 50, 22, HEADER},
        {2025, 2, 1285, 57, 22, HEADER},
        {2026, 2, 1399, 57, 24, HEADER},
        {2024, 3, 45, 0, 0, 0},
        {2025, 3, 45, 0, 0, 0},
        {2026, 3, 45, 0, 0, 0},
        {2024, 4, 1350, 60, 22, HEADER + 1},
        {2025, 4, 1284, 57, 22, HEADER + 1},
        {2026, 4, 1470, 60, 24, HEADER + 1},
        {2024, 7, 1239, 55, 22, HEADER},
        {2025, 7, 1239, 55, 22, HEADER},
        {2026, 7, 1445, 59, 24, HEADER},
        {2024, 11, 1460, 0, 0, 0},
        {2025, 11, 1460, 0, 0, 0},
        {2024, 8, 1020, 45, 22, HEADER + 1},
        {2025, 8, 1042, 46, 22, HEADER + 1},
    };
    if (!p || len < HEADER + 1)
        return nullptr;
    int format = p[0] | p[1] << 8;
    for (const Layout &l : table)
        if (l.format == format && l.id == p[6] && l.size == len)
            return &l;
    return nullptr;
}

struct Car {
    uint64_t lap_at = 0, status_at = 0, who_at = 0;
    uint8_t driver_status = 0, pit_timer = 0;
    uint32_t pit_lane_ms = 0, pit_stop_ms = 0;
    float sc_delta = 0;
    uint8_t drs_allowed = 0, pit_limiter = 0;
    int8_t fia_flag = 0;
    bool status_full = false;
    float drs_distance_m = 0;
    uint8_t race_number = 0;
    uint16_t team_id = 0, driver_id = 0;
    uint32_t team_colour = 0;
    bool who_full = false;
    uint64_t history_at = 0;
    uint32_t sector_best[3] = {};
};

struct Moments {
    std::vector<rc::events::Penalty> penalties;
    std::vector<rc::events::PenaltyServed> served;
    std::vector<rc::events::Classification> classifications;

    bool empty() const { return penalties.empty() && served.empty() && classifications.empty(); }
};

template <class E> E payload()
{
    E e{};
    e.size = uint32_t(sizeof(E));
    e.payload_version = E::version;
    return e;
}

class Source {
public:
    void reset()
    {
        uid_ = 0;
        session_at_ = 0;
        cars_ = {};
        reset_session();
    }

    // The live session ended: the values go, a final classification still to fire stays.
    void end()
    {
        auto keep = std::move(moments_.classifications);
        uint64_t uid = uid_;
        auto last = last_class_;
        reset();
        uid_ = uid;
        last_class_ = last;
        moments_.classifications = std::move(keep);
    }

    // Decodes one packet; false when it is not one this source reads.
    bool feed(const uint8_t *p, size_t len, uint64_t now_ms)
    {
        const Layout *l = layout(p, len);
        if (!l)
            return false;
        uint64_t uid = u64(p + 7);
        if (uid != uid_) {
            reset();
            uid_ = uid;
        }
        switch (l->id) {
        case 1:
            session(p, now_ms);
            break;
        case 2:
            lap_data(p, *l, now_ms);
            break;
        case 3:
            event(p, l->format);
            break;
        case 4:
            participants(p, *l, now_ms);
            break;
        case 7:
            car_status(p, *l, now_ms);
            break;
        case 11:
            history(p, now_ms);
            break;
        case 8:
            classification(p, *l);
            break;
        }
        return true;
    }

    // Fills what the memory read left clear in one snapshot's session, cars and details.
    void merge(SwoftyLiveSession &s, SwoftyLiveCar *cars, SwoftyLiveCarDetail *details, const bool *present,
               uint64_t now_ms) const
    {
        merge_session(s, now_ms);
        for (unsigned i = 0; i < CARS && i < SWOFTY_LIVE_MAX_CARS; i++)
            if (present[i])
                merge_car(cars_[i], cars[i], details[i], now_ms);
        fastest_lap(s, cars, details, present);
    }

    Moments take()
    {
        Moments out;
        out.penalties.swap(moments_.penalties);
        out.served.swap(moments_.served);
        out.classifications.swap(moments_.classifications);
        return out;
    }

private:
    void reset_session()
    {
        sc_phase_ = 0xff;
        sc_type_ = 0;
        drs_ = 0xff;
        drs_reason_ = 0xff;
        lights_ = 0xff;
        fastest_car_ = SWOFTY_LIVE_NO_CAR;
        fastest_ms_ = 0;
        last_class_.reset();
        moments_ = {};
    }

    void session(const uint8_t *p, uint64_t now)
    {
        session_at_ = now;
        total_laps_ = p[32];
        time_left_ = u16(p + 38);
        duration_ = u16(p + 40);
        sc_status_ = p[153];
        pit_ideal_ = p[682];
        pit_latest_ = p[683];
        pit_rejoin_ = p[684];
    }

    void lap_data(const uint8_t *p, const Layout &l, uint64_t now)
    {
        size_t base = l.stride == 50 ? 30 : 32;
        for (size_t i = 0; i < l.cars; i++) {
            const uint8_t *e = p + l.first + i * l.stride;
            Car &c = cars_[i];
            c.lap_at = now;
            c.driver_status = e[base + 12];
            c.pit_timer = e[base + 14];
            uint32_t lane = u16(e + base + 15), stop = u16(e + base + 17);
            if (c.pit_timer || lane || stop) {
                c.pit_lane_ms = lane;
                c.pit_stop_ms = stop;
            }
            c.sc_delta = f32(e + base - 4);
        }
    }

    void car_status(const uint8_t *p, const Layout &l, uint64_t now)
    {
        bool full = l.format != 2026;
        for (size_t i = 0; i < l.cars; i++) {
            const uint8_t *e = p + l.first + i * l.stride;
            Car &c = cars_[i];
            c.status_at = now;
            c.status_full = full;
            c.drs_allowed = e[22] == 1;
            c.fia_flag = int8_t(e[28]);
            c.pit_limiter = full ? e[4] : 0;
            c.drs_distance_m = full ? float(u16(e + 23)) : 0.f;
        }
    }

    void participants(const uint8_t *p, const Layout &l, uint64_t now)
    {
        size_t count = p[HEADER] < l.cars ? p[HEADER] : l.cars;
        for (size_t i = 0; i < count; i++) {
            const uint8_t *e = p + l.first + i * l.stride;
            Car &c = cars_[i];
            c.who_at = now;
            c.who_full = l.format != 2026;
            c.team_id = l.format == 2026 ? u16(e + 5) : e[3];
            c.driver_id = l.format == 2026 ? u16(e + 1) : e[1];
            c.race_number = l.format == 2026 ? 0 : e[5];
            c.team_colour = l.format == 2025 && e[44] ? uint32_t(e[45]) << 16 | uint32_t(e[46]) << 8 | e[47] : 0;
        }
    }

    void classification(const uint8_t *p, const Layout &l)
    {
        if (moments_.classifications.size() >= 2)
            return;
        auto e = payload<rc::events::Classification>();
        size_t count = p[HEADER] < l.cars ? p[HEADER] : l.cars;
        size_t at = l.stride == 46 ? 7 : 6;
        for (size_t i = 0; i < count; i++) {
            const uint8_t *r = p + l.first + i * l.stride;
            rc::events::ClassifiedCar &c = e.entries[e.cars++];
            c.car = uint32_t(i);
            c.position = r[0];
            c.laps = r[1];
            c.grid = r[2];
            c.points = r[3];
            c.pit_stops = r[4];
            c.status = r[5];
            c.reason = l.stride == 46 ? r[6] : 0;
            c.best_lap_ms = u32(r + at);
            double race_s;
            uint64_t bits = u64(r + at + 4);
            std::memcpy(&race_s, &bits, sizeof race_s);
            c.race_ms = race_s > 0 && race_s < 4e6 ? uint32_t(race_s * 1000.0 + 0.5) : 0;
            c.penalties_s = r[at + 12];
        }
        if (last_class_ && std::memcmp(&*last_class_, &e, sizeof e) == 0)
            return;
        last_class_ = e;
        moments_.classifications.push_back(e);
    }

    void history(const uint8_t *p, uint64_t now)
    {
        unsigned car = p[29], laps = p[30] < 100 ? p[30] : 100;
        if (car >= CARS)
            return;
        Car &c = cars_[car];
        c.history_at = now;
        for (unsigned n = 0; n < 3; n++) {
            unsigned lap = p[33 + n];
            c.sector_best[n] = 0;
            if (lap < 1 || lap > laps)
                continue;
            const uint8_t *at = p + 36 + (lap - 1) * 14 + 4 + 3 * n;
            c.sector_best[n] = uint32_t(u16(at)) + uint32_t(at[2]) * 60000u;
        }
    }

    uint32_t car_of(uint8_t idx) const { return idx < CARS ? idx : SWOFTY_LIVE_NO_CAR; }

    void event(const uint8_t *p, int format)
    {
        const char *code = reinterpret_cast<const char *>(p + 29);
        const uint8_t *d = p + 33;
        auto is = [&](const char *c) { return !std::memcmp(code, c, 4); };
        if (is("PENA") && d[2] < CARS && moments_.penalties.size() < 64) {
            auto e = payload<rc::events::Penalty>();
            e.penalty_type = d[0];
            e.infringement = d[1];
            e.car = d[2];
            e.other_car = car_of(d[3]);
            e.seconds = d[4];
            e.lap = d[5];
            e.places_gained = d[6];
            moments_.penalties.push_back(e);
        } else if ((is("DTSV") || is("SGSV")) && d[0] < CARS && moments_.served.size() < 64) {
            auto e = payload<rc::events::PenaltyServed>();
            e.car = d[0];
            e.kind = is("DTSV") ? rc::Served::drive_through : rc::Served::stop_go;
            e.stop_s = e.kind == rc::Served::stop_go && format >= 2025 ? f32(d + 1) : 0.f;
            moments_.served.push_back(e);
        } else if (is("FTLP") && d[0] < CARS) {
            fastest_car_ = d[0];
            fastest_ms_ = uint32_t(f32(d + 1) * 1000.f + 0.5f);
        } else if (is("DRSE")) {
            drs_ = 1;
            drs_reason_ = 0xff;
        } else if (is("DRSD")) {
            drs_ = 0;
            drs_reason_ = format >= 2025 && d[0] <= 3 ? d[0] : 0xff;
        } else if (is("STLG")) {
            lights_ = d[0] <= 5 ? d[0] : lights_;
        } else if (is("LGOT")) {
            lights_ = 0;
        } else if (is("SCAR") && d[0] <= 3 && d[1] <= 3) {
            sc_type_ = d[0];
            sc_phase_ = d[1];
        }
    }

    void merge_session(SwoftyLiveSession &s, uint64_t now) const
    {
        bool fresh = session_at_ && now - session_at_ <= STALE_MS;
        if (fresh && !(s.have & SWOFTY_LIVE_SHAVE_TIME_LEFT)) {
            s.time_left_s = float(time_left_);
            s.duration_s = float(duration_);
            s.have |= SWOFTY_LIVE_SHAVE_TIME_LEFT;
        }
        if (fresh && total_laps_ && !(s.have & SWOFTY_LIVE_SHAVE_TOTAL_LAPS)) {
            s.total_laps = total_laps_;
            s.have |= SWOFTY_LIVE_SHAVE_TOTAL_LAPS;
        }
        if (fresh && !(s.have & SWOFTY_LIVE_SHAVE_PIT_WINDOW)) {
            s.pit_window_ideal_lap = pit_ideal_;
            s.pit_window_latest_lap = pit_latest_;
            s.pit_rejoin_position = pit_rejoin_;
            s.have |= SWOFTY_LIVE_SHAVE_PIT_WINDOW;
        }
        bool formation = fresh && sc_status_ == 3;
        if (formation && (!(s.have & SWOFTY_LIVE_SHAVE_SAFETY_CAR) || s.safety_car == 0)) {
            s.safety_car = 3;
            s.have |= SWOFTY_LIVE_SHAVE_SAFETY_CAR;
        }
        if (sc_phase_ != 0xff && !(s.have & SWOFTY_LIVE_SHAVE_SAFETY_CAR_PHASE)) {
            s.safety_car_phase = sc_phase_;
            s.have |= SWOFTY_LIVE_SHAVE_SAFETY_CAR_PHASE;
        }
        if (drs_ != 0xff && !(s.have & SWOFTY_LIVE_SHAVE_DRS)) {
            s.drs_enabled = drs_;
            s.drs_reason = drs_reason_;
            s.have |= SWOFTY_LIVE_SHAVE_DRS;
        }
        if (lights_ != 0xff && !(s.have & SWOFTY_LIVE_SHAVE_START_LIGHTS)) {
            s.start_lights = lights_;
            s.have |= SWOFTY_LIVE_SHAVE_START_LIGHTS;
        }
    }

    static void mark(SwoftyLiveCar &c, SwoftyLiveCarDetail &d, uint64_t bit)
    {
        c.have |= bit;
        d.have |= bit;
    }

    static void merge_car(const Car &u, SwoftyLiveCar &c, SwoftyLiveCarDetail &d, uint64_t now)
    {
        if (u.lap_at && now - u.lap_at <= STALE_MS) {
            if (!(d.have & SWOFTY_LIVE_HAVE_DRIVER_STATUS)) {
                d.driver_status = u.driver_status;
                mark(c, d, SWOFTY_LIVE_HAVE_DRIVER_STATUS);
            }
            if (!(d.have & SWOFTY_LIVE_HAVE_PIT_TIMING)) {
                d.pit_lane_timer_active = u.pit_timer;
                d.pit_lane_ms = u.pit_lane_ms;
                d.pit_stop_ms = u.pit_stop_ms;
                mark(c, d, SWOFTY_LIVE_HAVE_PIT_TIMING);
            }
            if (!(d.have & SWOFTY_LIVE_HAVE_SC_DELTA)) {
                d.safety_car_delta_s = u.sc_delta;
                mark(c, d, SWOFTY_LIVE_HAVE_SC_DELTA);
            }
        }
        if (u.status_at && now - u.status_at <= STALE_MS) {
            if (!(d.have & SWOFTY_LIVE_HAVE_DRS_ALLOWED)) {
                d.drs_allowed = u.drs_allowed;
                d.drs_distance_m = u.drs_distance_m;
                mark(c, d, SWOFTY_LIVE_HAVE_DRS_ALLOWED);
            }
            if (!(d.have & SWOFTY_LIVE_HAVE_FIA_FLAG)) {
                d.fia_flag = u.fia_flag;
                mark(c, d, SWOFTY_LIVE_HAVE_FIA_FLAG);
            }
            if (u.status_full && !(d.have & SWOFTY_LIVE_HAVE_PIT_LIMITER)) {
                d.pit_limiter = u.pit_limiter;
                mark(c, d, SWOFTY_LIVE_HAVE_PIT_LIMITER);
            }
        }
        if (u.who_at && now - u.who_at <= PARTICIPANTS_STALE_MS && !(d.have & SWOFTY_LIVE_HAVE_IDENTITY)) {
            d.race_number = u.race_number;
            d.team_id = u.team_id;
            d.driver_id = u.driver_id;
            d.team_colour = u.team_colour;
            mark(c, d, SWOFTY_LIVE_HAVE_IDENTITY);
        }
        if (u.history_at && now - u.history_at <= PARTICIPANTS_STALE_MS && !(d.have & SWOFTY_LIVE_HAVE_SECTOR_BESTS)) {
            for (int n = 0; n < 3; n++)
                d.personal_best_sector_ms[n] = u.sector_best[n];
            mark(c, d, SWOFTY_LIVE_HAVE_SECTOR_BESTS);
        }
    }

    void fastest_lap(SwoftyLiveSession &s, const SwoftyLiveCar *cars, const SwoftyLiveCarDetail *details,
                     const bool *present) const
    {
        if (s.have & SWOFTY_LIVE_SHAVE_FASTEST_LAP)
            return;
        uint32_t best = 0, holder = SWOFTY_LIVE_NO_CAR;
        bool memory = false;
        for (unsigned i = 0; i < SWOFTY_LIVE_MAX_CARS; i++) {
            if (!present[i] || !(cars[i].have & SWOFTY_LIVE_HAVE_LAP_TIMES))
                continue;
            memory = true;
            uint32_t ms = details[i].best_lap_ms;
            if (ms && (!best || ms < best))
                best = ms, holder = i;
        }
        if (!memory && fastest_car_ != SWOFTY_LIVE_NO_CAR)
            best = fastest_ms_, holder = fastest_car_;
        if (!best)
            return;
        s.fastest_lap_ms = best;
        s.fastest_lap_vehicle = uint8_t(holder);
        s.have |= SWOFTY_LIVE_SHAVE_FASTEST_LAP;
    }

    uint64_t uid_ = 0, session_at_ = 0;
    uint8_t total_laps_ = 0, sc_status_ = 0, pit_ideal_ = 0, pit_latest_ = 0, pit_rejoin_ = 0;
    uint16_t time_left_ = 0, duration_ = 0;
    uint8_t sc_phase_ = 0xff, sc_type_ = 0, drs_ = 0xff, drs_reason_ = 0xff, lights_ = 0xff;
    std::optional<rc::events::Classification> last_class_;
    uint32_t fastest_car_ = SWOFTY_LIVE_NO_CAR, fastest_ms_ = 0;
    struct Cars {
        Car at[CARS];
        Car &operator[](size_t i) { return at[i]; }
        const Car &operator[](size_t i) const { return at[i]; }
    } cars_;
    Moments moments_;
};

}
