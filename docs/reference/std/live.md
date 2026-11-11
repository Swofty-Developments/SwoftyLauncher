---
title: Live state
description: Read every car, the classification, gaps, pit and penalty state and the track's turn numbers from the game's memory with swofty/live.hpp.
---

# Live state

The live library reads the running session from the game's memory: any car by vehicle id (the watched car too), the session header, the classification and road order, the gap to the leader and to the car in front, pit, tyre and penalty state, and the official turn at a car's lap distance. It needs no UDP output. A tower, a rival lane or a corner readout is a few lines.

```cpp [native/src/my_osd.cpp]
#include <swofty/events.hpp>
#include <swofty/live.hpp>
#include <swofty/mixin.hpp>

namespace live = swofty::live;

static void frame(double, void *)
{
    if (!live::available())
        return;
    auto me = live::subject(); // the watched car while the spectator HUD is up, else the player's
    if (!me)
        return;

    if (auto kmh = live::speed_kmh(*me))
        swofty::log("[my-osd] %.0f km/h, gear %d", *kmh, me->gear);

    if (auto turn = live::turn(*me)) // official turn near lap_m, 220 m ahead to 15 m behind
        swofty::log("[my-osd] turn %s in %.0f m", turn->label, turn->distance_ahead_m);

    live::Order order = live::order(); // the classification, leader first
    if (auto rival = order.ahead(me->vehicle)) { // P-1, or P2 for the leader
        if (auto gap = live::gap_ahead_ms(*me))
            swofty::log("[my-osd] P%d, +%.1f to car %u", me->position, *gap / 1000.0, rival->vehicle);
    }
}

SWOFTY_ON_LOAD
{
    swofty::events::on_frame(frame);
}

SWOFTY_ADDON()
```

```json [swofty.json]
{
  "schema_version": 1,
  "id": "author.my-osd",
  "name": "My OSD",
  "version": "1.0.0",
  "author": "Your name",
  "kind": ["native"],
  "compat": { "game": "f1_25" },
  "requires": [{ "id": "swofty.core", "version": ">=0.1.0" }],
  "experimental_features": ["telemetry"],
  "operations": { "native": [{ "dll": "native/my_osd.dll" }] }
}
```

- **Header:** `swofty/live.hpp`, namespace `swofty::live`; host test fake `swofty/fake_live.hpp`
- **Table:** the core export `SwoftyCoreLive` (`SwoftyLive`, table version 1); `SWOFTY_API_VERSION` is unchanged
- **Feature:** `telemetry`, experimental, off by default (the live state is the memory source of std telemetry)
- **Thread:** any thread; every call copies from the core's per-frame snapshot

## What a call answers

A car is the game's **vehicle id**, the slot of the game's vehicle table and the same index as `cars::live::Car::index()`. It is not the UDP participant index. The safety car is a vehicle too (`is_racer(c)` is false for it).

Every field has a bit in `have` that says the value was read this frame. A clear bit means the game did not give the value for that car, and the field is zero, never another car's value. Check `has()` before you draw a value.

```cpp
auto c = live::car(7);
if (c && live::has(*c, SWOFTY_LIVE_HAVE_ERS_STORE)) {
    float fill = *live::ers_fraction(*c); // store / capacity, 0 to 1
}
if (c && !live::has(*c, SWOFTY_LIVE_HAVE_SPEED))
    ; // the speed read failed for this car this frame: draw nothing
```

A call answers empty while no session is live, on a core without the live state, on a build whose layouts the core has no proof for, and for a group that is not verified for the running build. The core logs the groups it turned on and each group it leaves off once:

```text
[live] on (EA build): groups car systems, ers, world, aero, progress, classification, pit, penalties, flags, controllers, spectator, mode, badges, track, track name, spectator gap, bus events
[live] tyres off: VehicleTyreComponent, TyreSystem, TyreSlot, VisualCompound and PhysicalCompound fields (SWOFTY_LIVE_FORCE=1 reads it unverified)
[live] timing off on the EA build: k_Live_RelativeGapHelperKey does not resolve in this build's bindings
[live] pit lane off on the EA build: its layout is not proven there (SWOFTY_LIVE_FORCE=1 reads it anyway)
[live] off: build 1234567890 has no proven live layouts (Steam 1789118616, EA 1789128892)
```

## car

```c
typedef struct SwoftyLiveCar {
    uint32_t version, size;
    uint64_t have;                 // SWOFTY_LIVE_HAVE_*
    uint32_t vehicle;              // game vehicle id
    uint32_t participant;          // 0xffffffff
    uint8_t flags;                 // SWOFTY_LIVE_CAR_PLAYER, _AI_DRIVING, _WATCHED, _RACER
    uint8_t ers_class;             // 0 no ERS (safety car, every F2 car), 1 ERS
    float speed;                   // m/s
    float rpm, rpm_limit;
    int32_t gear;                  // -1 reverse, 0 neutral, 1..9
    float throttle, brake, clutch; // 0..1, after the reverse swap
    float steer;                   // -1 left .. +1 right, normalised to the lock
    float g_lat, g_long;
    float world_pos[3], world_vel[3];
    float ers_store, ers_capacity; // joules
    float ers_deploy, ers_harvest_k, ers_harvest_h, ers_max_deploy; // watts
    float ers_harvested_lap, ers_deployed_lap, ers_harvest_limit_lap; // joules
    int32_t ers_mode;              // 0 none, 1 medium, 2 overtake, 3 hotlap
    int32_t lap;                   // the game's counter, 0 on lap 1
    float lap_m, total_m;          // metres on this lap, and the session distance
    int32_t position;              // 1-based, 0 unknown
    uint8_t result_status;         // 2 running, 3 finished, 4 DNF, 5 DSQ, 6 NC, 7 retired
    uint8_t pit_state;             // the game's 0..15; 2..15 is in the pit
    uint8_t tyre_visual, tyre_actual;
    float time_penalty_s;
    int32_t gap_leader_ms, gap_ahead_ms; // SWOFTY_LIVE_NO_GAP when the game marks them invalid
    uint8_t drive_through, stop_go, warnings, track_limit_warnings;
    char tyre_letter;              // 'S' 'M' 'H' 'I' 'W', 0 unknown
    uint8_t sector;                // 1-based
    uint8_t drs;
    uint8_t pit_status;            // 0 on track, 1 in the pit lane, 2 in the pit box (SWOFTY_LIVE_HAVE_PIT_LANE)
    uint8_t pit_stops;             // completed stops
    uint32_t reserved[7];
} SwoftyLiveCar;
```

| Call | Answers |
|---|---|
| `live::car(vehicle)` | one car by vehicle id |
| `live::subject()` | the watched car while the spectator HUD is up, else the player's |
| `live::player()`, `live::watched()` | from `session().player` and `.watched` |
| `live::turn(car)` | the official turn near `car.lap_m` |

`speed_kmh`, `ers_fraction`, `gap_ahead_ms`, `gap_leader_ms`, `in_pit`, `pit_status` and `pit_stops` return `std::optional` and are empty when the value is not there.

Gaps follow the game's rule. They are valid in a race while the car is running or finished and not in the pit; a lapped car's gap is invalid, so compute "+1 LAP" from the lap counters. In practice, qualifying and Time Trial the game's gap system is not fed, so the gaps stay empty.

## session

```cpp
auto s = live::session();
if (s && live::has(*s, SWOFTY_LIVE_SHAVE_ORDER)) {
    for (uint32_t i = 0; i < s->classified; i++)
        swofty::log("[my-osd] P%u car %u", i + 1, s->order[i]);
}
```

```c
typedef struct SwoftyLiveSession {
    uint32_t version, size;
    uint64_t have;                 // SWOFTY_LIVE_SHAVE_*
    uint32_t type;                 // 1..4 practice, 5..9 qualifying, 10..14 shootout, 15..17 race, 18 Time Trial
    uint32_t cars, racers;         // cars in the table; competitors (not the safety car)
    int32_t player;                // vehicle id, -1 none (split screen reads as none)
    int32_t watched;               // vehicle id while the spectator HUD is up, else -1
    int32_t subject;               // watched if >= 0 else player
    uint32_t generation;           // bumps when the player's car or the car set changes
    uint32_t subject_generation;   // bumps on every spectator switch: reset smoothing on it
    float track_length_m;
    uint8_t safety_car;            // 0 none, 1 safety car, 2 virtual, 0xff unknown (3 is reserved; the game never sets it)
    uint8_t red_flag;              // 0 none, 1 deployed, 2 suspended
    uint8_t yellow_sectors;        // bit i: sector i + 1 has a yellow zone
    uint8_t chequered;
    uint8_t game_mode;             // SWOFTY_LIVE_MODE_GRAND_PRIX or _TIME_TRIAL
    uint8_t spectator_gap_mode;    // the game's raw value: 1 in session types 15-17, 3 otherwise
    char track_name[48];           // the game's internal track name
    float sector_end_m[8];
    uint32_t sector_count;
    uint32_t order[32];            // vehicle ids by classification, `classified` of them
    uint32_t classified;
    uint32_t road_order[32];       // vehicle ids by distance covered, `on_track` of them
    uint32_t on_track;
    uint32_t reserved[8];
} SwoftyLiveSession;
```

The watched car follows the spectator director only while the spectator HUD is up. The safety car cinematic and the winner sequence also switch the director on, and those do not count. A red flag is reported when the game's red flag system says so, or when every marshal zone reads red; the game's own byte stayed 0 through a red flag in a race, so the zones are the second witness. The safety car value is 0 while a red flag is out, because the game's byte also reads 1 then.

`live::order()` and `live::road_order()` give the two orders as vectors:

```cpp
live::Order o = live::order();
int mine = o.subject_rank;               // 0-based, -1 unknown
auto behind = o.behind(me->vehicle);     // the car in the next position
```

The classification is rejected whole when two cars report the same position (the game is re-sorting): `have` has no `ORDER` bit for that frame and every `position` is 0.

## turn

```cpp
for (const live::Turn &t : live::track_turns())
    swofty::log("[my-osd] turn %s %s at %.0f m", t.label, t.right ? "right" : "left", t.apex_lap_m);

auto c = live::car(7);
if (auto t = live::turn(*c))
    show(t->label, t->distance_ahead_m); // negative once the apex is behind, down to -15 m
```

```c
typedef struct SwoftyLiveTurn {
    uint32_t version, size;
    int32_t number;            // official turn number
    char letter;               // 'a' for 9a, else 0
    uint8_t right;             // 1 right, 0 left
    uint8_t source;            // SWOFTY_LIVE_TURN_OFFICIAL
    float distance_ahead_m;    // from lap_m to the apex
    float apex_lap_m;
    char label[8];             // "13", "9a"
    uint32_t reserved[4];
} SwoftyLiveTurn;
```

The table holds the 24 stock circuits' official turn numbers, picked by the game's internal track name and accepted only when the track length is within 3 % of the table's. With no usable name, a length within 1.5 % of exactly one circuit picks it. Reverse and short layouts, new tracks and modded tracks have no table, and `turn()` and `track_turns()` answer empty for them. The learned-bend fallback of the original is not part of the core yet.

The table is Project Halo's, by nyannoying ("Uses work from Project Halo by nyannoying", <https://github.com/nyannoying1337/f1-25-project-halo>); see `THIRD_PARTY.md` in the launcher repository.

## team_badge and frame

```cpp
if (auto key = live::team_badge(car.vehicle)) // "ferrari_2026"
    texture = "#overrides.osd.timing_tower.team_badges_hud." + *key;

auto f = live::frame(); // changes on every core refresh; poll it to skip unchanged frames
```

`team_badge` answers empty for My Team and for teams the game draws a rendered badge for.

## Test it on the host

`swofty/fake_live.hpp` installs a fake core table, so the code above runs without the game.

```cpp
#include <swofty/fake_live.hpp>

swofty::fake::Live fake;
fake.session().type = 15;
fake.add_car(0, 2).gap_ahead_ms = 950;
fake.car(0).have |= SWOFTY_LIVE_HAVE_GAP_AHEAD;
fake.add_car(1, 1);
fake.set_order({1, 0});
fake.set_player(0);
fake.set_turns({/* SwoftyLiveTurn rows */});
fake.install();

assert(swofty::live::gap_ahead_ms(*swofty::live::car(0)) == 950);
assert(swofty::live::order().ahead(0)->vehicle == 1);
```

## When a group reads

The core reads each group (car systems, ERS, progress, classification, timing, pit, pit lane, penalties, tyres, flags, controllers, spectator, game mode, track and the game's message bus) only when every site it needs resolves on the running build and its KB rows are verified. The two tests are the same one: the generated bindings export a site only for a row that is L2 or better with an independent `agree`, and the generated game headers declare a struct field under the same rule, so a group turns on by itself when its rows land and the headers are regenerated. The offsets the reader uses are tied to the generated structs with `static_assert`s, so a header that declares a field somewhere else fails the build.

A group also needs its layout proven for the running build. The Steam build is the one the knowledge base describes. The EA App build (PE timestamp 1789128892) runs every group, because each struct field the groups read was compared against the Steam code that reads it, and every site they need resolves in the EA bindings set. One displacement differs and the core applies it for you: the game manager's progress system is at `+0x2a0` on EA and at `+0x2d0` on Steam, so the progress group (and everything the core derives from lap distance) reads that field at the right place on each build. A build in neither column, such as a future patch, runs no group at all.

| group | Steam | EA App |
|---|---|---|
| car systems, ERS, world position, DRS bit | yes | yes |
| progress | yes | yes, with the progress system at `+0x2a0` |
| classification, timing, pit, pit lane, penalties, flags, controllers | yes | yes |
| spectator, spectator gap, track, track name | yes | yes |
| tyres, game mode, team badges | yes | yes |
| bus events (collision, retirement) | yes | yes |

A group that is yes still waits for its rows, as above, and the EA proof is static: it compares code, so run an EA race with `SWOFTY_TELEMETRY_VERIFY=1` once to compare the values with the game's UDP output. A group with any site that does not resolve stays off even with `SWOFTY_LIVE_FORCE=1`, which reads the groups whose sites resolve without waiting for verification, and on EA also without waiting for the layout proof; use it on a test machine to produce the evidence that verifies them, together with `SWOFTY_TELEMETRY_VERIFY=1` (see [Telemetry](./telemetry#memory-source)).

The player car comes from the controller system (the car whose active controller is the human one); `CarLive.is_ai` selects the physics rate and reads 1 for every car in a grid snapshot, so it is not used to find the player. With the controllers group off the player is unknown (`player` is -1) until it is.

A group whose chain the knowledge base has no verified row for reads only under `SWOFTY_LIVE_FORCE=1`.

The core also subscribes to the game's `r_car_to_car_col` and `vehicle_status_changed` bus messages (the bus group) to fire `telemetry.collision` and `telemetry.retirement` without UDP; see [Telemetry](./telemetry#events).

## Not read yet

Active aero mode, Boost and Overtake state, DRS allowed and distance, the pit limiter, driver name and team colour, the session clock and lap total, lap and sector times, and tyre temperatures have no verified memory source. `SWOFTY_LIVE_HAVE_BOOST`, `_AERO_MODE` and `_IDENTITY` are declared so the structs stay stable when they land and stay clear until then. The game's own view-model leaves carry most of these values, but the core has no leaf reader: the node's value layout is not in the knowledge base, so there is no `hud::get` yet.
