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
    uint32_t version;            // SWOFTY_TELEMETRY_VERSION (1)
    uint32_t session_type;
    uint32_t track;
    uint32_t flag;
    uint32_t weather;
    uint32_t total_laps;
    uint32_t participant_count;
    uint32_t player_index;       // your player's participant index, for car() and participant()
    float session_time;
    float session_time_left;
    float track_temp, air_temp;
    float reserved[8];           // room for new fields without changing the size
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
    float reserved[8];
} SwoftyCarState;                         // 192 bytes
```

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
    float reserved[4];
} SwoftyParticipant;          // 92 bytes
```

Each struct carries `version` = `SWOFTY_TELEMETRY_VERSION` (1), also `swofty::telemetry::version`, and ends in a `reserved` block so fields can be added without changing its size.

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

`telemetry` is experimental, and `experimental_features` in a manifest doesn't accept it yet; see [feature gates](/design/native-libraries#feature-gates). The core turns it on only when `features` in `swofty/assets.json` lists `telemetry`, and the build writes that list from the addons' `experimental_features`. So on current builds every call returns `unavailable`.

With the feature on, the core hooks the frame tick, the session start and the results finalise. A session is live from `Game_GameModule_OnSessionEnter` until the results are finalised; in it the feed's `frame` and `session.session_time` advance. The per-car struct is not catalogued yet, so the car and participant fields stay zero:

```text
[telemetry] on; frame tick hooked, session witnesses on/on; car and participant fields are not populated yet (per-car struct uncatalogued)
[telemetry] session started (Game_GameModule_OnSessionEnter); frame 18342
[telemetry] session ended (Game_SessionStateFinaliseResultsSync_OnResultsFinalised); frame 52817
```

See [the native-libraries design](/design/native-libraries#_3-telemetry).
