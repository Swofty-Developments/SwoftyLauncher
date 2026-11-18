---
title: Tyre damage
description: Each wheel's puncture state and deflation from swofty/systems/tyre_damage.hpp, and the car.tyre.damage event.
---

# Tyre damage

The tyre damage library gives each car's four wheels: whether the tyre is punctured or burst, and how far it has deflated. The core reads them through the [live state](./live) and fires `car.tyre.damage` when a wheel's state changes. A race director addon calls a car in, a HUD flashes the punctured corner, and a league addon logs every puncture with its lap.

```cpp [native/src/punctures.cpp]
#include <swofty/events.hpp>
#include <swofty/live.hpp>
#include <swofty/systems/tyre_damage.hpp>

namespace ev = swofty::events;
namespace live = swofty::live;
namespace tyres = swofty::tyre_damage;

SWOFTY_ON_LOAD
{
    ev::on<tyres::events::Damage>().after([](const tyres::events::Damage &e, const ev::Context &) {
        swofty::log("[punctures] vehicle %u %s %s (was %s), deflation %.2f, lap %d", e.car, tyres::name(e.wheel),
                    tyres::name(e.state), tyres::name(e.previous), e.deflation, e.lap);
    });
}

bool player_has_puncture()
{
    std::optional<live::Car> me = live::player();
    return me && tyres::damaged(*me).value_or(0) != 0;
}
```

```cpp
namespace swofty::tyre_damage {

enum class State : uint8_t { none = 0, punctured = 2, burst = 3, unknown = 0xff };
enum class Wheel : uint8_t { rear_left, rear_right, front_left, front_right };

std::optional<State> state(const live::Car &c, Wheel w);
std::optional<float> deflation(const live::Car &c, Wheel w);  // 0, 1 when a puncture starts, 2 at the burst
std::optional<uint8_t> damaged(const live::Car &c);            // bit i: Wheel(i) is punctured or burst
const char *name(State s);
const char *name(Wheel w);                                     // "RL", "RR", "FL", "FR"

namespace events {
struct Damage {                                                // notify, the flow thread
    static constexpr const char *point = "car.tyre.damage";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;                                              // the vehicle id
    Wheel wheel;
    State state;
    State previous;
    uint8_t reserved;
    float deflation;                                           // the wheel's deflation in the tick the change was read
    int32_t lap;                                               // the car's lap
};                                                             // 24 bytes
}

}
```

- **The state** is `CarWheel.tyre_flags` of each wheel: bit 1 is set when the game's tyre damage monitor adds a puncture, bit 0 (burst) once the deflation reaches 2 while the tyre is punctured. A tyre change clears both, and `car.tyre.damage` fires back to `none`.
- **The wheels** are the car's four at `CarSuspension +0x70..+0x88`, reached from the tyre component the live state already reads (VehicleTyreComponent +0x20, CarPhysicsRecord.car +0x58, CarLive.suspension +0x580).
- **The event** fires on the flow thread from the live tick, car by car and wheel by wheel, after the race control moments. The first read of a session fires nothing; a car whose read drops out is set again without an event when it comes back.
- **Empty** when the live state is off (the experimental feature `telemetry`), when its tyres group is off, on a build without the wheel puncture proof, or on a game header that does not export `CarWheel.tyre_flags` and `CarWheel.deflation`. `SWOFTY_LIVE_HAVE_TYRE_DAMAGE` is then clear and the event does not fire.
- **Builds:** the wheel chain is witnessed on the KB build and Steam 1.0.145 (0x142e88840, 0x142e92f70 and 0x144656f80 compare equal and use each offset at the same instruction). EA builds read nothing.

```text
[race-control] vehicle 7 FL tyre punctured (was none), deflation 1.00, lap 12
[race-control] vehicle 7 FL tyre burst (was punctured), deflation 2.00, lap 13
[race-control] vehicle 7 FL tyre none (was burst), deflation 0.00, lap 13
```
