---
title: Laps
description: The lap files the core records in Time Trial, and the two events around writing one.
---

# Laps

While Time Trial boards are on, the core records every completed Time Trial lap to a file under `swofty/laps/`. An addon reads those files, adds a section to one before it is written, or reacts when one lands.

The recorder runs when the launcher writes `"tt_boards": true` into `swofty/tt.json` and the session is a Time Trial. It samples the player's car once per rendered frame. A lap starts at a line crossing and ends at the next one. The lap out of the garage is not recorded. A lap is dropped when the car is reset or moved, when the session ends, or when it is too short or too long to be a lap.

On the build the core's lap offsets are from, the core also hooks the game's own lap record (`Sess_LapTimes_ReportNewLapTime`). A completed lap then waits up to two seconds after its line crossing for the game's lap result and, when the lap is valid, for the ghost blob the game commits on a personal best. The file is written when both have arrived or when the two seconds have passed. When the hook is not available the file is written at once and the lap's time comes from the clock. `SWOFTY_LAP_GAME_RESULT=0` turns the lap result off and `SWOFTY_LAP_GHOST=0` the ghost blob.

## The file

A lap is one file, `swofty/laps/20261110T041011Z-3.swlap`: the UTC time it was written and the lap's number in the session. It is written as a `.tmp` file and renamed.

```
0   8   magic  "SWLAP\0\1\0"
8   4   u32 header length
12  n   header, JSON, UTF-8
..      sections: 4-byte tag, u32 length, bytes
          "SMPL"  u32 count, u32 stride, then count * stride bytes
          "GHST"  the game's own ghost blob for the lap, when there is one (1 to 131072 bytes, copied verbatim)
          "ATST"  an attestation, appended by a laps.sealing handler
          "END\0" length 0
```

Numbers are little-endian. A reader skips a section whose tag it does not know.

The header names what the core knows and leaves out the rest:

```json
{
  "format": 1,
  "core": "1.8.38",
  "game_build": "<exe sha256>",
  "board": { "content": "swofty.swofty-2022/f1_2022", "track": "bahrain", "weather": "dry", "content_name": "Formula 1 2022" },
  "car": { "team": "game/mercedes", "vehicle": "game/mercedes_2025", "driver": "game/russell",
           "team_stat": 4, "vehicle_stat": 17, "livery_stat": 0 },
  "lap": { "time_ms": 91234, "sectors_ms": [29001, 31002, 31231], "valid": true, "source": "game", "lap_index": 3,
           "started_utc": "2026-11-10T04:10:11Z", "session_time_s": 412.5 },
  "assists": { "brk": 1, "abs": 1, "trc": 2, "drl": 0, "gbx": 3, "ers": 1, "input": 2 },
  "performance": "equal",
  "rate_hz": 60,
  "fields": [
    { "name": "t", "type": "f32", "unit": "s" },
    { "name": "px", "type": "f32", "unit": "m" },
    { "name": "gear", "type": "i8" }
  ],
  "nonce": "<the value of tt_nonce in tt.json>"
}
```

`lap.source` is `game` when the time is the game's own lap record and `clock` when the recorder measured it between two line crossings. `lap.valid` and `lap.sectors_ms` are present when the game's record gives them. The core takes the game's record only when the lap time is finite and between 5 seconds and 1 hour, every sector is positive, and the sectors add up to the lap time within 10 ms when the lap and all three sectors are valid. A record that fails these is logged and the lap's time comes from the clock.

These keys are left out when the core could not read them:

| Key | Meaning |
| --- | --- |
| `lap.session_time_s` | the core's telemetry session clock when the line was crossed, seconds since the session went live |
| `assists` | the raw assist bytes of the game's telemetry lap: `brk`, `abs`, `trc`, `drl`, `gbx` (3 is the game's automatic gearbox), `ers`, and `input` 0 pad, 1 wheel, 2 keyboard, 3 direct |
| `performance` | `realistic` or `equal` |
| `car.team_stat` `car.vehicle_stat` `car.livery_stat` | the game's stat ids of the player's participant |
| `board.content_name` | the title of the era box the content id names |

`board.weather` is guessed from the track wetness when the lap starts. The core has no read of the Time Trial weather setting. `board.game_board` and `board.track_name` are not written yet.

The ghost blob is built by the game on a personal best only, after the lap result. A lap's `GHST` section is the blob committed within two seconds of its line crossing, so a lap that is not a personal best has none.

A sample is the values of `fields`, in order, packed with no padding. `stride` is the sum of the field sizes. A reader parses samples from `fields` and nothing else, so the core adds a field without a new format number.

```cpp
const uint8_t *sample = smpl + 8 + i * stride;
float t;
std::memcpy(&t, sample + offset_of("t"), sizeof t);
```

The types are `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32` and `u32`. A field the core could not read in the running game is missing from `fields`.

| Field | Meaning |
| --- | --- |
| `t` | seconds since the lap started |
| `dist` | metres driven since the lap started |
| `px` `py` `pz` | world position, metres |
| `vx` `vy` `vz` | world velocity, metres per second |
| `qx` `qy` `qz` `qw` | orientation |
| `throttle` `brake` `steer` | pedals 0 to 1, steering -1 to 1 |
| `gear` `rpm` `speed` | gear (-1 reverse, 0 neutral), engine rpm, km/h |
| `ws_fl` `ws_fr` `ws_rl` `ws_rr` | wheel spin, radians per second |
| `slip_ang_*` `slip_ratio_*` | tyre slip angle and slip ratio per wheel |
| `surf_fl` `surf_fr` `surf_rl` `surf_rr` | the surface under each wheel (the game's surface id, 255 unknown) |
| `g_lat` `g_long` | the chassis g signals |
| `drs` | DRS wing open fraction |

## The integrity module

The launcher loads one more native addon for a session in which Time Trial boards is on and the player is signed in with Discord: `swofty.integrity`, a closed-source DLL that Swofty's server publishes. The launcher fetches it from `GET /v1/tt/integrity`, checks its size and sha256, and checks an Ed25519 signature over `swofty-tt-integrity-v1\0 || version || \0 || sha256` against the public key pinned in the launcher. A copy that passes is kept under `timetrial/integrity/<version>/` in the launcher's data folder, the newest two versions. The session places it as `swofty/addons/swofty.integrity/swofty_integrity.dll` and lists it in `swofty/settings.json` under `plugins`, and the session's rollback removes it with every other placed file.

It is not an addon in any profile, the addon list, the load order or the addon set a lap is uploaded with. It is named in `loader.log` as an addon DLL and in the launcher's session log:

```
integrity: swofty_integrity.dll 2026.11.10.1 under swofty/addons/swofty.integrity
integrity module: not loaded for this session: couldn't reach https://...
```

When the offer is missing, unreachable or fails a check, the session starts without the module and the lap is written without an `ATST` section.

At `laps.sealing` the module appends the `ATST` section: a length-prefixed JSON document and a 32-byte HMAC-SHA256 of it.

```
ATST = u32 json_len, json bytes, 32 bytes mac
json = { "v": 1, "module": "2026.11.10.1", "nonce": "...", "body_sha256": "...", "time_ms": 91234,
         "values": { ... }, "db": { ... }, "modules": [ { "name": "swofty_core.dll", "sha256": "..." } ], ... }
```

`body_sha256` is the hash of the lap file before the section, `values` and `db` are the handling values and database rows read for the player's car, and `modules` lists the DLLs loaded from the game folder. The key of the MAC is held by the server.

## Events

`laps.recorded` fires once the file is in place. `laps.sealing` fires just before, with the file as built so far.

```cpp
#include <swofty/systems/laps.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    events::on<laps::events::Recorded>().after([](const laps::events::Recorded &e, const events::Context &) {
        swofty::log("lap file %s, %d ms", std::string(e.path()).c_str(), e.time_ms);
    });

    events::on<laps::events::Sealing>().before([](laps::events::Sealing &e, const events::Context &) {
        uint8_t digest[32] = {};
        hash(e.body, e.body_len, digest);
        e.add_section("ATST", digest, sizeof digest);
    });
}
```

`Recorded::valid` is 1, 0, or -1 when the lap's validity is not known. `Sealing::add_section` refuses the tags `SMPL`, `GHST` and `END\0`, and the core takes at most four sections of at most 1 MiB together. The core adds the `END\0` section after them.

The recorder logs one line per lap in `swofty/loader.log`:

```
[laps] wrote 20261110T041011Z-3.swlap lap 91234 ms (game), 5474 samples, ghost 0 bytes, board game/f1_2025|bahrain|dry
```

The hooks log as they read:

```
[laps] lap result from the game: car 0 record 3 (message lap 3) 91234 ms, sectors 29001 31002 31231 ms, lap valid, sectors 111 valid
[laps] game lap extras: assists brk 1 abs 1 trc 2 drl 0 gbx 3 ers 1, input wheel, performance realistic
[laps] ghost blob from the game: 51284 bytes
```
