---
title: Race control
description: React to flags, the safety car, the red flag, sectors and retirements with the session.flag, session.safety_car.deploy, session.red_flag, session.sector and session.car.status events of swofty/systems/race_control.hpp.
---

# Race control

The race control library fires an event when the session's flag, the safety car, the red flag, a car's sector or a car's result status changes. The core reads them from the [live state](./live) once per frame, so an addon reacts to a yellow in sector 2, a virtual safety car or a retirement without reading memory or UDP itself.

```cpp [native/src/race_board.cpp]
#include <swofty/events.hpp>
#include <swofty/mixin.hpp>
#include <swofty/systems/race_control.hpp>

namespace rc = swofty::race_control;
namespace ev = swofty::events;

SWOFTY_ON_LOAD
{
    ev::on<rc::events::Flag>().after([](const rc::events::Flag &e, const ev::Context &) {
        if (e.flag == rc::Flag::yellow)
            swofty::log("[race-board] yellow in sectors 0x%x on lap %d", e.yellow_sectors, e.lap);
        else
            swofty::log("[race-board] %s flag (was %s)", rc::name(e.flag), rc::name(e.previous));
    });

    ev::on<rc::events::SafetyCar>().after([](const rc::events::SafetyCar &e, const ev::Context &) {
        if (e.deployment == rc::Deployment::virtual_safety_car)
            swofty::log("[race-board] VSC deployed on lap %d", e.lap);
        else if (e.deployment == rc::Deployment::none)
            swofty::log("[race-board] %s in", rc::name(e.previous));
    });

    ev::on<rc::events::RedFlag>().after([](const rc::events::RedFlag &e, const ev::Context &) {
        swofty::log("[race-board] red flag %s", rc::name(e.state));
    });

    ev::on<rc::events::CarStatus>().after([](const rc::events::CarStatus &e, const ev::Context &) {
        if (e.status == rc::Status::retired || e.status == rc::Status::did_not_finish)
            swofty::log("[race-board] vehicle %u out on lap %d", e.car, e.lap);
    });

    ev::on<rc::events::Sector>().after([](const rc::events::Sector &e, const ev::Context &) {
        if (e.sector == 1)
            swofty::log("[race-board] vehicle %u starts lap %d", e.car, e.lap);
    });
}

SWOFTY_ADDON()
```

```json [swofty.json]
{
  "schema_version": 1,
  "id": "author.race-board",
  "name": "Race Board",
  "version": "1.0.0",
  "author": "Your name",
  "kind": ["native"],
  "compat": { "game": "f1_25" },
  "requires": [{ "id": "swofty.core", "version": ">=0.1.0" }],
  "experimental_features": ["telemetry"],
  "operations": { "native": [{ "dll": "native/race_board.dll" }] }
}
```

`swofty/loader.log` during a race with a virtual safety car:

```text
[race-control] reading the live state: flag yes (marshal zones yes, red flag yes, classification yes), safety car yes, red flag yes, car status yes, sectors yes
[race-control] flag yellow, yellow sectors 0x2 (was green, 0x0), lap 14
[race-board] yellow in sectors 0x2 on lap 14
[race-control] safety car virtual safety car (was none), lap 14
[race-board] VSC deployed on lap 14
[race-control] vehicle 7 retired (was running), lap 14
[race-board] vehicle 7 out on lap 14
[race-control] safety car none (was virtual safety car), lap 16
[race-board] virtual safety car in
[race-control] flag green, yellow sectors 0x0 (was yellow, 0x2), lap 16
[race-board] green flag (was yellow)
```

- **Header:** `swofty/systems/race_control.hpp`, namespace `swofty::race_control`
- **Events:** `session.flag`, `session.safety_car.deploy`, `session.red_flag`, `session.sector` (hot), `session.car.status`; all notify, through the hooks table (`SwoftyApi` version 9, `events.hpp`)
- **Feature:** `events` (stable) and `telemetry` (experimental, the live state)
- **Thread:** the flow thread, once per frame after the live state's tick
- **Changes nothing in the game:** it reads the live state and fires events

## How a moment fires

Each frame the core compares the live snapshot with the one before and fires one event per change, in this order: red flag, safety car, flag, then for each car by vehicle id its result status and its sector.

- **The first read of a session fires nothing.** It sets the starting state, so a race that starts under green fires no green flag, and an addon that wants the state at the start reads `race_control::now()`.
- **A session starts again** when the live snapshot is not valid (no session), or the session type, the car set (`SwoftyLiveSession::generation`), the game mode or the track changes. Its first read fires nothing.
- **A value the live state cannot read is forgotten.** A frame with its `have` bit clear fires nothing, and the next frame that reads it sets the state again without an event. A build where a live group is off gives no event for what that group reads (see [Live state: groups and builds](./live)).

```cpp
// The state now, without waiting for an event: empty while no session is live or the live state is off.
if (auto st = rc::now()) {
    swofty::log("[race-board] %s, yellow 0x%x, %s, red flag %s, leader on lap %d", rc::name(st->flag), st->yellow_sectors,
                rc::name(st->safety_car), rc::name(st->red_flag), st->lap);
}

// The same reads on one live snapshot or car you already have.
if (auto s = swofty::live::session()) {
    rc::State st = rc::read(*s);                  // lap stays 0: pass the leader's lap as the second argument
}
if (auto car = swofty::live::car(7)) {
    std::optional<rc::Status> status = rc::status(*car); // empty without the classification bit
    std::optional<uint8_t> sector = rc::sector(*car);    // 1-based; empty without the sector bit
}
```

## The flag

```cpp
enum class Flag : uint8_t {
    unknown = SWOFTY_FLAG_NONE, // none of the marshal zones, red flag or classification is read
    green = SWOFTY_FLAG_GREEN,
    yellow = SWOFTY_FLAG_YELLOW, // a sector has a yellow zone
    red = SWOFTY_FLAG_RED,
    chequered = 5,               // the leader has finished
};

struct Flag {                   // race_control::events::Flag, "session.flag", version 1
    uint32_t size, payload_version;
    race_control::Flag flag;
    race_control::Flag previous;
    uint8_t yellow_sectors;          // bit i: sector i + 1 has a yellow zone
    uint8_t previous_yellow_sectors;
    int32_t lap;                     // the leader's lap, 0 while it is not read
};
```

The flag is red while the red flag is out, chequered once the leader has finished, yellow while any sector has a yellow zone and green otherwise. The event fires when the flag or the set of yellow sectors changes, so a second yellow sector fires yellow to yellow with both sets. A yellow after the chequered flag keeps the flag chequered and fires the new set. Blue flags are per car and not part of it.

Addons made for 1.5 that subscribe to `flag` (`SWOFTY_EVENT_FLAG`) get the same moment: `e->a` is the flag (the numbers above) and `e->b` the yellow sectors, with no `data`.

```cpp
static void on_flag(const SwoftyEvent *e, void *)
{
    if (e->a == SWOFTY_FLAG_YELLOW)
        swofty::log("[old-addon] yellow in sectors 0x%llx", (unsigned long long)e->b);
}
swofty::events::subscribe(SWOFTY_EVENT_FLAG, on_flag);
```

## The safety car and the red flag

```cpp
using Deployment = live::SafetyCar; // none, safety_car, virtual_safety_car, formation, unknown

struct SafetyCar {              // race_control::events::SafetyCar, "session.safety_car.deploy", version 1
    uint32_t size, payload_version;
    Deployment deployment;      // none: the safety car is in
    Deployment previous;
    int32_t lap;
};

enum class RedFlag : uint8_t { none = 0, deployed = 1, suspended = 2, unknown = 0xff };

struct RedFlag {                // race_control::events::RedFlag, "session.red_flag", version 1
    uint32_t size, payload_version;
    race_control::RedFlag state; // none: withdrawn
    race_control::RedFlag previous;
    int32_t lap;
};
```

`session.safety_car.deploy` fires when the safety car is deployed, when the virtual safety car becomes the safety car (or back), and when it comes in. The game never sets `formation`. Under a red flag the live state reads the safety car as none, so a red flag shown during a safety car fires the red flag, then the safety car in, then the flag red.

## Sectors and result status

```cpp
struct Sector {                 // race_control::events::Sector, "session.sector", version 1 (hot)
    uint32_t size, payload_version;
    uint32_t car;               // vehicle id
    uint8_t sector;             // 1-based
    uint8_t previous;
    int32_t lap;                // the car's lap
};

enum class Status : uint8_t { invalid, inactive, running, finished, did_not_finish, disqualified, not_classified, retired };

struct CarStatus {              // race_control::events::CarStatus, "session.car.status", version 1
    uint32_t size, payload_version;
    uint32_t car;
    Status status;
    Status previous;
    int32_t lap;
};
```

`session.sector` fires up to once per car per sector, so it is hot: the core logs only its first fire of a session. Crossing the line fires sector 1 with the new lap. A sector the live state reads as 0 fires nothing. `session.car.status` fires for each car whose result status changes: the leader finishing, a retirement, a disqualification, a car not classified at the end. Each status change is in `swofty/loader.log`.

## Scope and selectors

The events carry the session's context like every other event: `in()` and `when()` narrow a handler to a box, a track or a career mode.

```cpp
ev::on<rc::events::SafetyCar>()
    .in(ev::track("game/spa"))
    .after([](const rc::events::SafetyCar &e, const ev::Context &) {
        swofty::log("[race-board] Spa safety car: %s", rc::name(e.deployment));
    });
```

## Testing with the fake live state

`race_control::now()`, `read()`, `status()` and `sector()` read through `swofty/live.hpp`, so the fake live state answers them on the host:

```cpp
#include <swofty/fake_live.hpp>
#include <swofty/systems/race_control.hpp>
#include <cassert>

int main()
{
    swofty::fake::Live fake;
    fake.session().have |= SWOFTY_LIVE_SHAVE_YELLOW | SWOFTY_LIVE_SHAVE_SAFETY_CAR;
    fake.session().yellow_sectors = 0x4;
    fake.session().safety_car = 2;
    SwoftyLiveCar &lead = fake.add_car(5, 1);
    lead.have |= SWOFTY_LIVE_HAVE_PROGRESS;
    lead.lap = 12;
    fake.set_order({5});
    fake.install();

    auto st = swofty::race_control::now();
    assert(st && st->flag == swofty::race_control::Flag::yellow && st->lap == 12);
    assert(st->safety_car == swofty::race_control::Deployment::virtual_safety_car);
}
```

## Builds

The library reads the live state's flags group (marshal zones, red flag, sector ends), timing (the safety car), classification (result status, the chequered flag) and progress (each car's sector and lap). On Steam 1.0.145 (build 26566308) and on EA every one of them is on; EA's live state turns off only the driver names and the rendered badges. A group that is off leaves the events that need it silent, and the `[race-control] reading the live state:` line at the first read of a session says which.

The five points are live only while the live state is on: with the `telemetry` feature off, or on a build without proven live layouts, `swofty::hooks::live<rc::events::Flag>()` is false and no race control event or 1.x `flag` fires.

```cpp
#include <swofty/hooks.hpp>

if (!swofty::hooks::live<rc::events::SafetyCar>())
    swofty::log("[race-board] no race control here: turn on telemetry, or read the UDP feed yourself");
```
