---
title: Laps
description: The lap files the core records in Time Trial, and the two events around writing one.
---

# Laps

While Time Trial boards are on, the core records every completed Time Trial lap to a file under `swofty/laps/`. An addon reads those files, adds a section to one before it is written, or reacts when one lands.

The recorder runs when the launcher writes `"tt_boards": true` into `swofty/tt.json` and the session is a Time Trial. It samples the player's car once per rendered frame. A lap starts at a line crossing and ends at the next one. The lap out of the garage is not recorded. A lap is dropped when the car is reset or moved, when the session ends, or when it is too short or too long to be a lap.

## The file

A lap is one file, `swofty/laps/20261110T041011Z-3.swlap`: the UTC time it was written and the lap's number in the session. It is written as a `.tmp` file and renamed.

```
0   8   magic  "SWLAP\0\1\0"
8   4   u32 header length
12  n   header, JSON, UTF-8
..      sections: 4-byte tag, u32 length, bytes
          "SMPL"  u32 count, u32 stride, then count * stride bytes
          "GHST"  the game's own ghost blob for the lap, when there is one
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
  "board": { "content": "game/f1_2025", "track": "bahrain", "weather": "dry" },
  "car": { "team": "game/mercedes", "vehicle": "game/mercedes_2025", "driver": "game/russell" },
  "lap": { "time_ms": 91234, "source": "clock", "lap_index": 3, "started_utc": "2026-11-10T04:10:11Z" },
  "rate_hz": 60,
  "fields": [
    { "name": "t", "type": "f32", "unit": "s" },
    { "name": "px", "type": "f32", "unit": "m" },
    { "name": "gear", "type": "i8" }
  ],
  "nonce": "<the value of tt_nonce in tt.json>"
}
```

`lap.source` is `game` when the time is the game's own lap record and `clock` when the recorder measured it between two line crossings. `lap.valid` and `lap.sectors_ms` are present when the game's record gives them.

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
[laps] wrote 20261110T041011Z-3.swlap lap 91234 ms, 5474 samples, board game/f1_2025|bahrain|dry
```
