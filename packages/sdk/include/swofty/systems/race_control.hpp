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

}

}
