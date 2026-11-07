---
title: Telemetry
description: Read live F1 25 car and session state, or open the shared-memory feed, with swofty/telemetry.hpp.
---

# Telemetry

The telemetry library reads live car and session state into typed structs, and can publish the same data in a shared-memory feed for external tools.

```cpp [native/src/my_plugin.cpp]
#include <swofty/events.hpp>
#include <swofty/mixin.hpp>
#include <swofty/telemetry.hpp>

namespace tel = swofty::telemetry;

static void frame(double, void *)
{
    if (!tel::available()) // false outside a live session, and while the feature is off
        return;
    tel::SessionState s{};
    tel::CarState car{};
    // the session gives the player's participant index for the car read
    if (tel::session(&s) == tel::Result::ok && tel::car(s.player_index, &car) == tel::Result::ok)
        swofty::log("[my-addon] %.0f km/h, gear %d", car.speed * 3.6f, car.gear); // speed is m/s
}

SWOFTY_ON_LOAD
{
    const char *feed = nullptr;
    tel::Result r = tel::shared_memory(&feed); // turn on the feed for an external tool
    swofty::log("[my-addon] telemetry feed: %s %s", tel::name(r), feed ? feed : "");
    swofty::events::on_frame(frame);
}

SWOFTY_ADDON()
```

```json [swofty.json]
{
  "schema_version": 1,
  "id": "author.my-addon",
  "name": "My Addon",
  "version": "1.0.0",
  "author": "Your name",
  "kind": ["native"],
  "compat": { "game": "f1_25" },
  "requires": [{ "id": "swofty.core", "version": ">=0.1.0" }],
  "operations": { "native": [{ "dll": "native/my_plugin.dll" }] }
}
```

`swofty/loader.log` on current builds, where the feature is off:

```text
[my-addon] telemetry feed: unavailable
```

- **Header:** `swofty/telemetry.hpp`, namespace `swofty::telemetry`
- **Table:** `SwoftyApi::telemetry` (`SwoftyTelemetry`), API version 5
- **Feature:** `telemetry`, experimental, off by default
- **Thread:** the flow thread

Every call returns `swofty::telemetry::Result` (`ok`, `late`, `unavailable`, `invalid`); see the [result codes](./#result-codes). Each read gives you the most recent frame.

## available

```cpp
if (tel::available()) { ... } // the feature is on, implemented, and a session is live
```

## session

```cpp
tel::SessionState s{};
tel::session(&s); // ok and fills s; unavailable outside a session; invalid for a null out
```

```c
typedef struct SwoftySessionState {
    uint32_t version;            // SWOFTY_TELEMETRY_VERSION (2)
    uint32_t session_type;
    uint32_t track;
    uint32_t flag;               // SWOFTY_FLAG_*: 0 none, 1 green, 2 blue, 3 yellow, 4 red
    uint32_t weather;
    uint32_t total_laps;
    uint32_t participant_count;
    uint32_t player_index;       // your player's participant index, for car() and participant()
    float session_time;
    float session_time_left;
    float track_temp, air_temp;
    // version 2, filled from the UDP packets
    uint32_t safety_car_status;  // 0 none, 1 safety car, 2 virtual safety car, 3 formation lap
    uint32_t track_length;       // metres
    uint32_t udp_format;         // 2024, 2025 or 2026 once a packet has been decoded; 0 before
    uint32_t packets_seen;       // bit n set once packet id n has been decoded
    uint32_t pit_speed_limit;    // km/h
    uint32_t red_flag;           // 1 from a red flag event until the next lights out
    uint32_t reserved[2];
} SwoftySessionState;            // 80 bytes
```

## car

```cpp
tel::CarState car{};
tel::car(s.player_index, &car); // ok; invalid for an index of 24 or more, or a null out
```

```c
typedef struct SwoftyCarState {
    uint32_t version;
    uint32_t flags;
    float speed;                          // m/s
    float rpm;
    float max_rpm;
    int32_t gear;
    float throttle, brake, clutch, steer;
    float tyre_temp[4];                   // per wheel
    float tyre_wear[4];
    float brake_temp[4];
    float fuel;
    float fuel_laps;
    int32_t position;
    uint32_t lap;
    uint32_t sector;
    float last_lap_time, current_lap_time, best_lap_time;
    float sector_time[3];
    float lap_distance;
    float world_pos[3];
    float world_vel[3];
    // version 2
    uint8_t tyre_compound;                // actual compound id: 16 C5, 17 C4, 18 C3, 19 C2, 20 C1, 21 C0, 22 C6, 7 inter, 8 wet
    uint8_t tyre_visual;                  // visual compound id: 16 soft, 17 medium, 18 hard, 7 inter, 8 wet
    uint8_t tyre_age_laps;
    uint8_t pit_status;                   // 0 on track, 1 pitting, 2 in the pit area
    uint8_t result_status;                // 0 invalid, 1 inactive, 2 active, 3 finished, 4 did not finish, 5 disqualified, 6 not classified, 7 retired
    uint8_t driver_status;                // 0 in garage, 1 flying lap, 2 in lap, 3 out lap, 4 on track
    uint8_t pit_stops;
    uint8_t warnings;                     // total warnings
    uint8_t penalty_seconds;
    uint8_t drs;                          // 1 while open
    uint8_t grid_position;
    uint8_t ai_controlled;
    uint8_t drive_through_pens, stop_go_pens, corner_cut_warnings;
    int8_t fia_flag;                      // -1 invalid, 0 none, 1 green, 2 blue, 3 yellow
    uint32_t gap_front_ms;                // to the car in front
    uint32_t gap_leader_ms;
    float ers_store;                      // joules
    float reserved;
} SwoftyCarState;                         // 192 bytes
```

`flags` carries `SWOFTY_CAR_FLAG_*` bits:

| Bit | Name | Set when |
|---|---|---|
| 0 | `LAP_DATA` | a Lap Data packet has filled the car |
| 1 | `STATUS` | a Car Status packet has filled the car |
| 2 | `IN_PIT_LANE` | `pit_status` is 1 or 2 |
| 3 | `RETIRED` | `result_status` is 4 or 7 |
| 4 | `DISQUALIFIED` | `result_status` is 5 |
| 5 | `FINISHED` | `result_status` is 3 |
| 6 | `IN_GARAGE` | `driver_status` is 0 |
| 7 | `DRS_ALLOWED` | the game allows DRS |
| 8 | `LAP_INVALID` | the current lap is invalid |

`tyre_temp`, `tyre_wear` and `brake_temp` use the game's wheel order: rear left, rear right, front left, front right. `tyre_temp` is the surface temperature in degrees, `tyre_wear` is percent, `speed` is m/s, `clutch` is 0 to 1.

## participant

```cpp
tel::Participant who{};
tel::participant(3, &who); // ok; invalid for an index of 24 or more, or a null out
swofty::log("[my-addon] %s is P%d", who.name, who.position);
```

```c
typedef struct SwoftyParticipant {
    uint32_t version;
    uint32_t flags;
    char name[48];
    uint32_t team;
    int32_t position;
    uint32_t lap;
    float lap_distance;
    float best_lap_time;
    // version 2
    uint32_t driver_id;           // the game's driver id
    uint8_t race_number, ai_controlled, nationality, my_team;
    float reserved[2];
} SwoftyParticipant;          // 92 bytes
```

Each struct carries `version` = `SWOFTY_TELEMETRY_VERSION` (2), also `swofty::telemetry::version`, and ends in a `reserved` block so fields can be added without changing its size. Version 2 fills part of each `reserved` block; the sizes and every version 1 offset are unchanged, so a version 1 reader keeps working.

## Events

The core fires these on the flow thread, from the game's own Event packets. Subscribe with [`swofty::events::subscribe`](./events); `a` and `b` are participant indices, and there is no `data`.

```cpp
swofty::events::subscribe(tel::event::overtake, [](const SwoftyEvent *e, void *) {
    swofty::log("[my-addon] car %lld passed car %lld", (long long)e->a, (long long)e->b);
});
```

| Constant | Event | `a` | `b` |
|---|---|---|---|
| `SWOFTY_EVENT_TELEMETRY_OVERTAKE` | `telemetry.overtake` | the overtaking car | the overtaken car |
| `SWOFTY_EVENT_TELEMETRY_COLLISION` | `telemetry.collision` | first car | second car |
| `SWOFTY_EVENT_TELEMETRY_RETIREMENT` | `telemetry.retirement` | the car | the retirement reason from the packet (0 when the format has none) |

The events need the `events` feature and the game's UDP telemetry output on.

`SwoftySessionState.flag` takes the `SWOFTY_FLAG_*` values and `SwoftyCarState.flags` the `SWOFTY_CAR_FLAG_*` bits listed above; both come from the game's UDP packets, so they stay zero while its UDP telemetry output is off. See [Availability](#availability).

## Shared-memory feed

`shared_memory(&name)` turns on the feed for external tools. It opens a named file mapping, `Local\SwoftyTelemetry`, and writes its name. The name stays valid for the lifetime of the process.

```text
[telemetry] shared_memory: Local\SwoftyTelemetry mapped, 6920 bytes
```

Inside the mapping is a `SwoftyTelemetryFeed`:

```c
typedef struct SwoftyTelemetryFeed {
    uint32_t version;                     // offset 0: SWOFTY_TELEMETRY_VERSION
    uint32_t size;                        // offset 4: sizeof(SwoftyTelemetryFeed), 6920
    uint64_t seq;                         // offset 8: seqlock; odd while a frame is being written
    uint64_t frame;                       // offset 16: frame counter
    SwoftySessionState session;           // offset 24: the current session
    SwoftyCarState cars[24];              // offset 104: per participant
    SwoftyParticipant participants[24];   // offset 4712: per participant
} SwoftyTelemetryFeed;
```

Use the sequence counter to check whether a frame was written while you were copying:

```cpp
// an external tool, reading the mapping
HANDLE m = OpenFileMappingW(FILE_MAP_READ, FALSE, L"Local\\SwoftyTelemetry");
auto *feed = static_cast<const SwoftyTelemetryFeed *>(MapViewOfFile(m, FILE_MAP_READ, 0, 0, sizeof(SwoftyTelemetryFeed)));

SwoftyTelemetryFeed copy;
for (;;) {
    uint64_t before = feed->seq;                // 1. seq must be even
    if (before & 1)
        continue;
    memcpy(&copy, feed, sizeof copy);           // 2. copy the fields you need
    if (feed->seq == before)                    // 3. unchanged: the copy is one whole frame
        break;                                  //    changed: retry
}
```

## UDP telemetry ids

The game's own UDP telemetry (formats 2024, 2025 and 2026) reports the stat id of every team and driver. A build with added teams or drivers writes `swofty/udp_ids.json`, and the core rewrites those ids in the outgoing packets.

```json [swofty/udp_ids.json]
{
  "version": 1,
  "teams": {"507": 4, "518": 1},
  "drivers": {"197": 255, "198": 8},
  "teams_byte": {"251": 4, "6": 1},
  "drivers_byte": {"197": 255, "198": 8},
  "lines": ["team 507 -> 4 (forceindia)", "driver 197 -> 255 (no stock driver)"]
}
```

```text
[udp] stock ids on: 2 team id(s), 2 driver id(s), 1 sendto slot(s)
[udp] stock ids: team 507 -> 4 (forceindia); driver 197 -> 255 (no stock driver)
[udp] first telemetry packet rewritten: format 2025, packet 4, 2025 model: driver 197 -> 255, team 251 -> 4
```

The rule:

- An added team reports its stock ancestor's stat id, found by following the `add-row` `from` chain to a row the game ships.
- In the 2026 format an added team reports the 2026 stock team of its ancestor's slot, 476 + the 2025 id (476 Mercedes, 477 Ferrari, 478 Red Bull, 479 Williams, 480 Aston Martin, 481 Alpine, 482 RB, 483 Haas, 484 McLaren, 485 Audi, 486 Cadillac). When the session's Participants or Lobby Info packet shows only the 2025 teams (ids 0-9), it keeps the 2025 id.
- An added driver reports its stock ancestor's id when `first_name`, `last_name` and `full_name` equal the ancestor's. Otherwise it reports 255, the spec's network human, and overlays show the driver by `m_name`. A driver id above 254 with no stock ancestor reports 255.
- Stock rows keep their ids.
- The 2024 and 2025 formats carry one byte per id. An added id whose low byte a stock row or another mapped id shares is not rewritten in those formats.

The core changes a buffer only when it parses as an F1 packet: `m_packetFormat` is 2024, 2025 or 2026, `m_packetId` is one below, and the length is that packet's size in that format.

```text
Participants (4)   2024 1350 B, 2025 1284 B: driverId, teamId (u8 per entry)
                   2026 1470 B: driverId, teamId (u16 per entry)
Lobby Info (9)     2024 1306 B, 2025 954 B: teamId in each entry (u8)
                   2026 1062 B: teamId in each entry (u16)
Time Trial (14)    2024 and 2025 101 B: teamId in each of the three data sets (u8)
                   2026 104 B: teamId in each of the three data sets (u16)
```

No other packet in any of the three formats carries a team or driver id.

```text
[udp] format 2025 packet 14 is 104 bytes, not the 101 the id rewrite expects; left alone
[udp] first time trial packet: format 2025, team ids 29, 0, 0, rewritten
```

The core logs the first packet of each format and id whose length differs from the table, and the team ids of the first Time Trial packet it sees.

`SWOFTY_UDP_STOCK_IDS=0` turns the rewrite off, for a tool that reads the real ids through an addon's registry `stat_id`.

## Availability

`telemetry` is experimental and off unless an addon asks for it. List it in the addon's `swofty.json`; while the addon is enabled, the build writes `telemetry` into `features` in `swofty/assets.json`, and the core turns the feature on. Addons that don't list it leave it off, and every call returns `unavailable`.

```json [swofty.json]
"experimental_features": ["telemetry"]
```

The game's UDP telemetry output must also be on in its settings for the car, participant and session fields to fill (see below).

With the feature on, the core hooks the frame tick, the session start and the results finalise. A session is live from `Game_GameModule_OnSessionEnter` until the results are finalised; in it the feed's `frame` and `session.session_time` advance.

The car, participant and session fields come from the packets the game builds for its UDP telemetry, read in the `sendto` hook before the id rewrite. The game sends them only while UDP telemetry output is on in its settings; with it off the fields stay zero, `session.udp_format` stays 0 and no event fires. The decode does not use the UDP port and does not change the packet.

```text
Session (1)        2024, 2025 753 B: weather, temperatures, laps, track length, marshal zone flags, safety car status
Lap Data (2)       2024 1131 B, 2025 1285 B, 2026 1399 B: times, gaps, position, pit and result status, penalties
Event (3)          45 B in all three formats: OVTK, COLL, RTMT, RDFL, GLGT
Participants (4)   2024 1350 B, 2025 1284 B: names, team, driver id; 2026 1470 B: team and driver id only
Car Telemetry (6)  2024, 2025 1352 B: speed, pedals, steer, gear, rpm, brake and tyre temperatures
Car Status (7)     2024, 2025 1239 B: fuel, tyre compound and age, ERS store, DRS allowed
Car Damage (10)    2024, 2025 1041 B: tyre wear
```

A packet whose size is not the listed one is ignored. In the 2026 format the Session, Car Telemetry, Car Status and Car Damage packets have other sizes whose field offsets are not catalogued, so only Lap Data, Event and the ids from Participants fill; compound, fuel, speed and wear stay zero there.

`session.flag` is the highest flag in any marshal zone (yellow over blue over green), or red from a red flag event until the next lights out. The safety car's vehicle index is not in the game's packets, and neither are the per-lap best times.

```text
[telemetry] on; frame tick hooked, session witnesses on/on; car, participant and session fields fill from the game's UDP packets while its UDP telemetry output is on
[udp] sendto hooked for the telemetry feed, 2 slot(s); no id rewrite
[telemetry] session started (Game_GameModule_OnSessionEnter); frame 18342
[telemetry] first UDP packet 2 decoded: format 2025, 1285 bytes
[telemetry] session ended (Game_SessionStateFinaliseResultsSync_OnResultsFinalised); frame 52817
```

For a car's speed without UDP in the meantime, see [Car speed without UDP](/std/systems/failures#car-speed-without-udp).

See [the native-libraries design](/design/native-libraries#_3-telemetry).
