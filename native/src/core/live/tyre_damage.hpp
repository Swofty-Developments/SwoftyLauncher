#pragma once
#include <swofty/systems/tyre_damage.hpp>

#include <cstdint>
#include <string_view>
#include <vector>

#include "core/live/live_snapshot.hpp"
#include "core/live/race_control.hpp"

// Tyre damage from the live snapshot (std lib tyre_damage, swofty/systems/tyre_damage.hpp): the tracker keeps each
// car's last read wheel states and turns each new snapshot into the wheels that changed, car by car, wheel by wheel. A
// session starts again, and its first read fires nothing, when the snapshot is not valid or its session key changes
// (race_control::session_key). A car whose tyre damage have bit is clear is forgotten and set again, without a moment,
// when it comes back. Pure: the live tick (race_control.cpp) feeds it and fires what it returns.

namespace swofty_core::live::tyre_damage {

namespace td = swofty::tyre_damage;

inline bool fires(std::string_view point) { return point == td::events::Damage::point; }

inline td::State wheel_state(uint8_t v)
{
    return v == SWOFTY_LIVE_TYRE_PUNCTURED || v == SWOFTY_LIVE_TYRE_BURST ? td::State(v) : td::State::none;
}

class Tracker {
public:
    void reset()
    {
        primed_ = false;
        for (CarSeen &c : cars_)
            c = CarSeen{};
    }

    bool primed() const { return primed_; }
    uint64_t sessions() const { return sessions_; }

    std::vector<td::events::Damage> step(const Snapshot *s)
    {
        std::vector<td::events::Damage> out;
        if (!s || !s->valid) {
            reset();
            return out;
        }
        race_control::SessionKey k = race_control::session_key(*s);
        if (!primed_ || !(k == key_)) {
            reset();
            key_ = k;
            primed_ = true;
            sessions_++;
        }
        for (uint32_t id = 0; id < SWOFTY_LIVE_MAX_CARS; id++) {
            CarSeen &seen = cars_[id];
            const SwoftyLiveCar &c = s->cars[id];
            if (!s->present[id] || !(c.have & SWOFTY_LIVE_HAVE_TYRE_DAMAGE)) {
                seen = CarSeen{};
                continue;
            }
            int32_t lap = (c.have & SWOFTY_LIVE_HAVE_PROGRESS) ? c.lap : 0;
            for (unsigned w = 0; w < td::WHEELS; w++) {
                td::State now = wheel_state(c.tyre_damage[w]);
                if (seen.known && now != seen.state[w]) {
                    auto e = race_control::payload<td::events::Damage>();
                    e.car = id;
                    e.wheel = td::Wheel(w);
                    e.state = now;
                    e.previous = seen.state[w];
                    e.deflation = c.tyre_deflation[w];
                    e.lap = lap;
                    out.push_back(e);
                }
                seen.state[w] = now;
            }
            seen.known = true;
        }
        return out;
    }

private:
    struct CarSeen {
        bool known = false;
        td::State state[td::WHEELS] = {};
    };

    bool primed_ = false;
    uint64_t sessions_ = 0;
    race_control::SessionKey key_;
    CarSeen cars_[SWOFTY_LIVE_MAX_CARS];
};

}
