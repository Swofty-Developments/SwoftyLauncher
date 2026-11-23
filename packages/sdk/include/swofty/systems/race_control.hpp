// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#include <optional>
#include <swofty/events.hpp>
#include <swofty/live.hpp>

// Race control: the session's flag, the safety car, the red flag, each car's sector and result status. The core
// reads them from its live state (swofty/live.hpp) once per frame and fires an event when one changes:
// session.flag, session.safety_car.deploy, session.red_flag, session.sector and session.car.status. Every event is
// notify, fires on the flow thread, and needs the live state (the experimental feature telemetry) on a build where
// the group that reads it is on. The first read of a session sets the starting state and fires nothing; a value the
// live state cannot read for a frame is forgotten, and the next read sets it again without an event. read() and
// now() give the current state without waiting for an event.

namespace swofty::race_control {

// The session's flag. 0 to 4 are the SWOFTY_FLAG_* numbers of swofty_api.h; blue flags are per car and not here.
enum class Flag : uint8_t {
    unknown = SWOFTY_FLAG_NONE,
    green = SWOFTY_FLAG_GREEN,
    yellow = SWOFTY_FLAG_YELLOW,
    red = SWOFTY_FLAG_RED,
    chequered = 5,
};

enum class RedFlag : uint8_t { none = 0, deployed = 1, suspended = 2, unknown = 0xff };

using Deployment = live::SafetyCar;

// A car's result status, the game's numbers (the UDP m_resultStatus ones).
enum class Status : uint8_t {
    invalid = 0,
    inactive = 1,
    running = 2,
    finished = 3,
    did_not_finish = 4,
    disqualified = 5,
    not_classified = 6,
    retired = 7,
};

inline const char *name(Flag f)
{
    switch (f) {
    case Flag::green:
        return "green";
    case Flag::yellow:
        return "yellow";
    case Flag::red:
        return "red";
    case Flag::chequered:
        return "chequered";
    default:
        return "unknown";
    }
}

inline const char *name(RedFlag r)
{
    switch (r) {
    case RedFlag::none:
        return "none";
    case RedFlag::deployed:
        return "deployed";
    case RedFlag::suspended:
        return "suspended";
    default:
        return "unknown";
    }
}

inline const char *name(Deployment d)
{
    switch (d) {
    case Deployment::none:
        return "none";
    case Deployment::safety_car:
        return "safety car";
    case Deployment::virtual_safety_car:
        return "virtual safety car";
    case Deployment::formation:
        return "formation";
    default:
        return "unknown";
    }
}

inline const char *name(Status s)
{
    switch (s) {
    case Status::inactive:
        return "inactive";
    case Status::running:
        return "running";
    case Status::finished:
        return "finished";
    case Status::did_not_finish:
        return "did not finish";
    case Status::disqualified:
        return "disqualified";
    case Status::not_classified:
        return "not classified";
    case Status::retired:
        return "retired";
    default:
        return "invalid";
    }
}

// The race control state of one live session read. flag is red while the red flag is out, chequered once the leader
// has finished, yellow while a sector has a yellow zone and green otherwise; it is unknown while the live state reads
// none of the marshal zones, the red flag or the classification. yellow_sectors has bit i set while sector i + 1 has a
// yellow zone. lap is the leader's lap, 0 while the classification or the leader's progress is not read.
struct State {
    Flag flag = Flag::unknown;
    uint8_t yellow_sectors = 0;
    Deployment safety_car = Deployment::unknown;
    RedFlag red_flag = RedFlag::unknown;
    int32_t lap = 0;
};

inline Flag flag_of(const live::Session &s)
{
    if (live::has(s, SWOFTY_LIVE_SHAVE_RED_FLAG) && s.red_flag != 0)
        return Flag::red;
    if (live::has(s, SWOFTY_LIVE_SHAVE_ORDER) && s.chequered)
        return Flag::chequered;
    if (live::has(s, SWOFTY_LIVE_SHAVE_YELLOW))
        return s.yellow_sectors ? Flag::yellow : Flag::green;
    return Flag::unknown;
}

inline State read(const live::Session &s, int32_t leader_lap = 0)
{
    State st;
    st.flag = flag_of(s);
    st.yellow_sectors = live::has(s, SWOFTY_LIVE_SHAVE_YELLOW) ? s.yellow_sectors : 0;
    st.safety_car = live::safety_car(s);
    st.red_flag =
        live::has(s, SWOFTY_LIVE_SHAVE_RED_FLAG) && s.red_flag <= 2 ? static_cast<RedFlag>(s.red_flag) : RedFlag::unknown;
    st.lap = leader_lap;
    return st;
}

inline std::optional<Status> status(const live::Car &c)
{
    return live::has(c, SWOFTY_LIVE_HAVE_CLASS) ? std::optional<Status>(static_cast<Status>(c.result_status)) : std::nullopt;
}

inline std::optional<uint8_t> sector(const live::Car &c)
{
    return live::has(c, SWOFTY_LIVE_HAVE_SECTOR) && c.sector ? std::optional<uint8_t>(c.sector) : std::nullopt;
}

// The state now, from the live snapshot; empty while no session is live or the live state is off.
inline std::optional<State> now()
{
    auto s = live::session();
    if (!s)
        return std::nullopt;
    int32_t lap = 0;
    if (live::has(*s, SWOFTY_LIVE_SHAVE_ORDER) && s->classified && s->order[0] < live::max_cars)
        if (auto lead = live::car(s->order[0]); lead && live::has(*lead, SWOFTY_LIVE_HAVE_PROGRESS))
            lap = lead->lap;
    return read(*s, lap);
}

// The penalty a car serves (session.penalty.served).
enum class Served : uint8_t { drive_through = 1, stop_go = 2 };

namespace events {

// The session's flag or its set of yellow sectors changes (notify). The 1.x name flag fires with it, with
// e->a the flag and e->b the yellow sectors.
struct Flag {
    static constexpr const char *point = "session.flag";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    race_control::Flag flag;
    race_control::Flag previous;
    uint8_t yellow_sectors;
    uint8_t previous_yellow_sectors;
    int32_t lap;
};

// The safety car changes (notify): deployed, switched between the safety car and the virtual safety car, or in
// (deployment none). Under a red flag the live state reads the safety car as none.
struct SafetyCar {
    static constexpr const char *point = "session.safety_car.deploy";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    Deployment deployment;
    Deployment previous;
    int32_t lap;
};

// The red flag changes (notify): deployed, suspended, or withdrawn (state none).
struct RedFlag {
    static constexpr const char *point = "session.red_flag";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    race_control::RedFlag state;
    race_control::RedFlag previous;
    int32_t lap;
};

// A car's sector changes (notify, hot: up to one per car per sector). sector is 1-based; lap is the car's lap.
struct Sector {
    static constexpr const char *point = "session.sector";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    uint8_t sector;
    uint8_t previous;
    int32_t lap;
};

// A car's result status changes (notify): finished, retired, disqualified, not classified. car is the vehicle id.
struct CarStatus {
    static constexpr const char *point = "session.car.status";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    Status status;
    Status previous;
    int32_t lap;
};

// Race control issues a penalty or a warning (notify), from the game's UDP Event packet PENA. car and other_car
// are vehicle ids (other_car SWOFTY_LIVE_NO_CAR when none); penalty_type and infringement are the game's numbers
// (see swofty::race_control::penalty_name and infringement_name); seconds is the time added, lap the lap it was given
// on.
struct Penalty {
    static constexpr const char *point = "session.penalty";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    uint32_t other_car;
    uint8_t penalty_type;
    uint8_t infringement;
    uint8_t seconds;
    uint8_t places_gained;
    int32_t lap;
};

// A car serves a drive through or a stop and go (notify), from DTSV and SGSV. stop_s is the time stationary (format
// 2025; 0 otherwise).
struct PenaltyServed {
    static constexpr const char *point = "session.penalty.served";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    Served kind;
    uint8_t pad[3];
    float stop_s;
    int32_t lap;
};

// Race control enables or disables DRS (notify). reason is the session's drs_reason.
struct Drs {
    static constexpr const char *point = "session.drs";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint8_t enabled;
    uint8_t reason;
    uint8_t pad[2];
    int32_t lap;
};

// A start light comes on (lights 1 to 5) or the lights go out (lights 0, out 1) (notify).
struct StartLights {
    static constexpr const char *point = "session.start_lights";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint8_t lights;
    uint8_t out;
    uint8_t pad[2];
    int32_t lap;
};

// The safety car's phase changes (notify): SWOFTY_LIVE_SC_DEPLOYED, _RETURNING (in this lap), _RETURNED or
// _RESUME. deployment is the deployment it belongs to; previous is 0xff for the first phase of a session.
struct SafetyCarPhase {
    static constexpr const char *point = "session.safety_car.phase";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    Deployment deployment;
    uint8_t phase;
    uint8_t previous;
    uint8_t pad;
    int32_t lap;
};

// One car of the final classification. car is the vehicle id; status is the result status (Status numbers) and
// reason the game's result reason (format 2025, else 0); race_ms is the race time without penalties.
struct ClassifiedCar {
    uint32_t car;
    uint8_t position, laps, grid, points;
    uint8_t pit_stops, status, reason, penalties_s;
    uint32_t best_lap_ms;
    uint32_t race_ms;
};

// The game publishes the session's final classification (notify), from the UDP Final Classification packet: cars
// entries, by vehicle id.
struct Classification {
    static constexpr const char *point = "session.classification";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t cars;
    ClassifiedCar entries[24];
};

// The session's fastest lap changes hands or gets faster (notify). car and previous_car are vehicle ids
// (previous_car SWOFTY_LIVE_NO_CAR for the first fastest lap); lap is the holder's lap.
struct FastestLap {
    static constexpr const char *point = "session.fastest_lap";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;
    uint32_t lap_ms;
    uint32_t previous_car;
    uint32_t previous_ms;
    int32_t lap;
};

}

// The game's penalty type numbers (the UDP PENA penaltyType ones).
inline const char *penalty_name(uint8_t type)
{
    static const char *const names[] = {"drive through", "stop go", "grid penalty", "penalty reminder", "time penalty",
                                        "warning", "disqualified", "removed from formation lap", "parked too long timer",
                                        "tyre regulations", "this lap invalidated", "this and next lap invalidated",
                                        "this lap invalidated without reason", "this and next lap invalidated without reason",
                                        "this and previous lap invalidated", "this and previous lap invalidated without reason",
                                        "retired", "black flag timer"};
    return type < sizeof names / sizeof *names ? names[type] : "unknown";
}

// The game's infringement type numbers (the UDP PENA infringementType ones).
inline const char *infringement_name(uint8_t type)
{
    static const char *const names[] = {"blocking by slow driving", "blocking by wrong way driving",
                                        "reversing off the start line", "big collision", "small collision",
                                        "collision failed to hand back position single", "collision failed to hand back position multiple",
                                        "corner cutting gained time", "corner cutting overtake single",
                                        "corner cutting overtake multiple", "crossed pit exit lane", "ignoring blue flags",
                                        "ignoring yellow flags", "ignoring drive through", "too many drive throughs",
                                        "drive through reminder serve within n laps", "drive through reminder serve this lap",
                                        "pit lane speeding", "parked for too long", "ignoring tyre regulations",
                                        "too many penalties", "multiple warnings", "approaching disqualification",
                                        "tyre regulations select single", "tyre regulations select multiple",
                                        "lap invalidated corner cutting", "lap invalidated running wide",
                                        "corner cutting ran wide gained time minor", "corner cutting ran wide gained time significant",
                                        "corner cutting ran wide gained time extreme", "lap invalidated wall riding",
                                        "lap invalidated flashback used", "lap invalidated reset to track",
                                        "blocking the pitlane", "jump start", "safety car to car collision",
                                        "safety car illegal overtake", "safety car exceeding allowed pace",
                                        "virtual safety car exceeding allowed pace", "formation lap below allowed speed",
                                        "formation lap parking", "retired mechanical failure", "retired terminally damaged",
                                        "safety car falling too far back", "black flag timer", "unserved stop go penalty",
                                        "unserved drive through penalty", "engine component change", "gearbox change",
                                        "parc ferme change", "league grid penalty", "retry penalty", "illegal time gain",
                                        "mandatory pitstop", "attribute assigned"};
    return type < sizeof names / sizeof *names ? names[type] : "unknown";
}

inline const char *phase_name(uint8_t phase)
{
    switch (phase) {
    case SWOFTY_LIVE_SC_DEPLOYED:
        return "deployed";
    case SWOFTY_LIVE_SC_RETURNING:
        return "returning";
    case SWOFTY_LIVE_SC_RETURNED:
        return "returned";
    case SWOFTY_LIVE_SC_RESUME:
        return "resume race";
    default:
        return "none";
    }
}

}
