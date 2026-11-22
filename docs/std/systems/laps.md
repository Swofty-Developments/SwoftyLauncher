---
title: Laps
description: The lap files the Time Trial plugin records, and the two events around writing one.
---

# Laps

While Time Trial boards are on and the player is signed in with Discord, every completed Time Trial lap is recorded to a file under `laps/` in the launcher's data folder. When the data folder can't be written, the file goes under `swofty/laps/` in the game folder instead. An addon reads those files, adds a section to one before it is written, or reacts when one lands.

The recorder is part of the [Time Trial plugin](#the-time-trial-plugin). It runs when the launcher writes `"tt_boards": true` into `swofty/tt.json` and the session is a Time Trial. It samples the player's car at 60 Hz. A lap starts at a line crossing and ends at the next one. The lap out of the garage is not recorded. A lap is dropped when the car is reset or moved, when the session ends, or when it is too short or too long to be a lap.

A completed lap waits up to two seconds after its line crossing for the game's lap result and, when the lap is valid, for the ghost blob the game commits on a personal best. The file is written when both have arrived or when the two seconds have passed. Without the game's lap result the lap's time comes from the clock. `SWOFTY_LAP_GAME_RESULT=0` turns the lap result off and `SWOFTY_LAP_GHOST=0` the ghost blob.

## The file

A lap is one file, `laps/20261110T041011Z-3.swlap`: the UTC time it was written and the lap's number in the session. It is written as a `.tmp` file and renamed. A write that fails in the data folder goes to `swofty/laps/`, and the loader log names the folder and the Windows error code of the failed write. The launcher collects laps from both folders. On an EA app copy under `C:\Program Files\EA Games` the game can't create files in `swofty/`.

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
  "input": { "v": 1, "game": { "device": "controller" }, "core": "ok", "devices": [] },
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
| `board.track_name` | the track's display name from the core's track base |
| `board.track_addon` `board.track_kind` | on a custom track, the addon id that adds or fills the track and `record` or `older`; left out on a stock track |

### The input record

The Time Trial plugin writes `input`: the game's own input class of the car through the lap, and the game controllers and virtual pad reads the core lists at the lap's start and end. A sealed lap carries it inside the body its attestation hashes.

```json
"input": {
  "v": 1,
  "game": { "device": "controller", "start": "controller", "end": "controller",
            "samples": { "controller": 362, "wheel": 0, "keyboard": 0, "direct": 0 }, "switches": 0 },
  "core": "ok",
  "pad": { "from": "lap", "owners": [], "seen": ["title"], "set": 0, "idle": 0, "real": 5431 },
  "devices": [
    { "kind": "xinput", "slot": 0, "subtype": "gamepad", "vid": "045e", "pid": "028e" },
    { "kind": "gamepad", "usage": "01:05", "vid": "045e", "pid": "028e", "name": "Controller (XBOX 360 For Windows)", "xinput": true },
    { "kind": "joystick", "usage": "01:04", "vid": "046d", "pid": "c24f", "name": "G29 Driving Force Racing Wheel" }
  ],
  "devices_total": 3, "keyboards": 1, "mice": 1, "devices_age_s": 1.3
}
```

| Key | Meaning |
| --- | --- |
| `game` | the car's input class, read every 250 ms of the lap: the class with the most reads, the first and last read, the reads per class and how often it changed |
| `core` | `ok`, `none` from a core without the lap host's `input` (before version 4), `refused` when the core turned the call down; `pad` and `devices` are there only with `ok` |
| `pad` | the core's virtual pad: who holds it now (`owners`) and who held it since the game started (`seen`): `title` (title skip and direct start), `agent` (the test agent), `plugin` (an addon); and the XInput slot 0 reads through the lap it answered with held buttons or triggers (`set`), with nothing held (`idle`) and from the real controller (`real`). `from` is `process` when the lap's start had no answer and the counts run from the game's start |
| `devices` | up to 16 game controllers at the lap's end: the XInput slots (`slot`, `subtype` as XInput names it or its number) and the Raw Input HID joysticks, gamepads and multi-axis controllers (`usage` page:usage), with the USB vendor and product ids and the product string when Windows has them. `virtual` marks the core's virtual pad in slot 0 when no real controller is there, `xinput` the HID entry of an XInput device |
| `devices_total` `keyboards` `mice` | every game controller listed, and the keyboards and mice |
| `devices_age_s` | how old the core's device list is; the core reads it every 5 s while a plugin asks. Left out with `devices` before the first read |

The core logs its first read and every change of the controllers:

```
[input] read the input devices: 2 game controller(s) (1 XInput, 1 named), 1 keyboard(s), 1 mouse/mice in 4 ms
[input] the game controllers changed; read the input devices: 3 game controller(s) (1 XInput, 2 named), 1 keyboard(s), 1 mouse/mice in 3 ms
```

and the plugin one line per lap:

```
[laps] input: lap 3 game controller (362 controller, 0 wheel, 0 keyboard, 0 direct samples, 0 switches); core ok: 3 controller(s) listed, pad set 0 idle 0 real 5431
```

`board.weather` is guessed from the track wetness when the lap starts. The core has no read of the Time Trial weather setting. `board.game_board` is the last board name the game's leaderboard screen requested in the session, written when its weather matches the lap's. `ghost_id` is the base64 of the 0x30-byte id block of the last committed personal best, written on laps that carry a `GHST` section. Both keys are left out when unknown.

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

## Custom tracks

A track an addon adds, or a dormant stock row an addon fills, is a custom track. The core reads the session's track code against its track base (`apply::tracks::Base`, filled at boot) and writes `board.track` as:

- a stock track: the code, `bahrain`
- a custom track, kind `record` or `older`: `<addon id>:<code>`, lowercased, `swofty.tracks:ring`

The key has a colon and never a slash. The game's own season (`game/f1_2025`) and every addon season carry the key. When two rows share a code, the track number of the game's last board name picks the row. A code the base does not list, or a base that is not ready, gives the lowercased code with no `board.track_addon` and `board.track_kind`, and the core logs once:

```
[laps] track base: no track with this code; the board key is the track code 'ring' (logged once)
```

```json
"board": { "content": "game/f1_2025", "track": "swofty.tracks:ring", "weather": "dry", "track_name": "The Ring",
           "track_addon": "swofty.tracks", "track_kind": "record" }
```

The mailbox request's `board.track` carries the same key. The core takes the request's track from the board name's track number through the base's stat ids, and from the session's track when the base has no such stat id.

A board is the core's own when its descriptor is 6 or higher or when its track is custom. The request redirect and the guards use that one rule. A request for a stock descriptor on a custom track goes to the mailbox like an addon board, and with guards on a lap on a custom track in the game's own season does not reach the publisher's boards.

## The Time Trial plugin

The launcher places `swofty.tt` for a session in which the player is signed in with Discord: a closed-source DLL that Swofty's server publishes. The launcher reads the offer from `GET /v1/modules`, downloads the file from the CDN link in the offer when the offer has one (`https`, no sign-in sent) and from `GET /v1/modules/file` otherwise or when the CDN copy doesn't arrive or doesn't match, and checks its size, its sha256 and an Ed25519 signature over `swofty-module-v1\0 || name || \0 || platform || \0 || version || \0 || abi || \0 || sha256` against the public key pinned in the launcher. A copy that passes is kept under `modules/tt-game/<platform>/<version>/` in the launcher's data folder. When Swofty's server answers a download with `too many requests` or a server error, times out or can't be reached, the launcher uses the newest cached copy that still verifies. The session places it as `swofty/addons/swofty.tt/swofty_tt.dll`, and the session's rollback removes it with every other placed file. It is not an addon in any profile.

The plugin attaches to the core's lap host, `SwoftyCoreLapHost` in `swofty_api.h`:

```
[tt] attached to the core's lap host (version 1)
module tt-game: not loaded for this session: none published
```

From lap host version 4 the plugin calls the host's `input` at each lap's start and end for the lap's `input` record:

```c
SwoftyLapInput in = { .size = sizeof in };
if (host->version >= 4 && host->size >= SWOFTY_LAP_HOST_V3_SIZE + sizeof(void *) && host->input(&in) == 1)
    for (uint32_t i = 0; i < in.count; i++)
        log("%s %04x:%04x", in.devices[i].name, in.devices[i].vendor, in.devices[i].product);
```

`input` answers 0 for a NULL table or one shorter than `SwoftyLapInput`. It reads nothing from the game.

Without the plugin no lap is recorded and the in-game boards are off.

The plugin records laps and runs the in-game boards on Steam F1 25 1.0.141 and 1.0.145.2472 and on the EA app's 1.0.141 and 1.0.145. On any other build of the game, a game update included, `loader.log` says `[boards] off: ...` and, in a Time Trial, `[laps] NO LAP IS RECORDED IN THIS SESSION: ...`.

## The integrity module

The launcher loads one more native addon for a session in which Time Trial boards is on and the player is signed in with Discord: `swofty.integrity`, a closed-source DLL that Swofty's server publishes. The launcher reads the offer from `GET /v1/tt/integrity`, downloads the file from the CDN link in the offer when the offer has one and from `GET /v1/tt/integrity/file` otherwise, checks its size and sha256, and checks an Ed25519 signature over `swofty-tt-integrity-v1\0 || version || \0 || sha256` against the public key pinned in the launcher. A copy that passes is kept under `timetrial/integrity/<version>/` in the launcher's data folder with the offer it came with, the newest two versions. The session places it as `swofty/addons/swofty.integrity/swofty_integrity.dll` and lists it in `swofty/settings.json` under `plugins`, and the session's rollback removes it with every other placed file.

The integrity module and the Time Trial plugin are fetched the same way. When Swofty's server times out, can't be reached, answers `too many requests` or a server error, the launcher uses the newest cached copy whose kept offer and bytes still verify; for the integrity module, only one the server offered in the last 14 days. With no such copy, the launcher asks again after 1 and 2 seconds, within 12 seconds in all.

It is not an addon in any profile, the addon list, the load order or the addon set a lap is uploaded with. It is named in `loader.log` as an addon DLL and in the launcher's session log:

```
integrity: swofty_integrity.dll 2026.11.10.1 under swofty/addons/swofty.integrity
integrity module: asking for the offer failed (couldn't reach https://...); trying again in 1 s (try 2 of 3)
integrity module: the server's copy could not be had (couldn't reach https://...); using the cached copy 2026.11.10.1, verified offline
integrity module: not loaded for this session: couldn't reach https://...
```

When the offer is missing, unreachable or fails a check and no cached copy can stand in, the session starts without the module and the lap is written without an `ATST` section. The launcher then says so in a dialog at Play: Time Trial laps that session won't be sealed and won't rank.

Each lap's upload meta says what the session did about the module, under `integrity`:

```json
{"state": "loaded", "version": "2026.11.10.1", "source": "cdn"}
{"state": "loaded", "version": "2026.11.10.1", "source": "cache", "detail": "couldn't reach https://..."}
{"state": "loaded", "version": null, "source": "env"}
{"state": "fetch_failed", "version": null, "source": null, "detail": "couldn't reach https://...: timeout: global"}
```

`state` is `loaded`, `no_offer` (the server answered 404), `fetch_failed` (no answer and no cached copy), `bad_offer` (the signature, size or sha256 did not verify), `signed_out` (the server did not take the sign-in) or `refused` (any other refusal, in the server's words). `source` says where the bytes came from: `server`, `cdn`, `cache` or `env` (`SWOFTY_TT_INTEGRITY_DLL`). `detail` is at most 200 characters. A launcher that does not know leaves `integrity` out.

At `laps.sealing` the module appends the `ATST` section: a length-prefixed JSON document and a 32-byte HMAC-SHA256 of it.

```
ATST = u32 json_len, json bytes, 32 bytes mac
json = { "v": 1, "module": "2026.11.10.1", "nonce": "...", "body_sha256": "...", "time_ms": 91234,
         "values": { ... }, "db": { ... }, "modules": [ { "name": "swofty_core.dll", "sha256": "..." } ], ... }
```

`body_sha256` is the hash of the lap file before the section, `values` and `db` are the handling values and database rows read for the player's car, and `modules` lists the DLLs loaded from the game folder. The key of the MAC is held by the server.

## Server-timed laps

At each timing line of a lap (the start line, sector 1, sector 2 and the finish) the Time Trial plugin calls the lap host's `mark` with the lap's number, the line and the game's lap clock there. The plugin numbers each lap at its start line: 1 for the first lap and one more for each lap started. A discarded lap keeps its number, so the `lap_index` of the lap files has gaps after a discard. The number is the lap file's `lap_index`. The core stamps the moment and writes one file per mark into `tt/ticks/` of the launcher's data folder (`swofty/tt/ticks/` when it can't write there):

```json
{"seq": 12, "lap": 3, "mark": "s1", "game_ms": 28411, "at_ms": 1795000123456}
```

While the game runs, the launcher sends each mark, in the order they were made, to `POST /v1/tt/sessions/{nonce}/ticks` on a thread of its own:

```json
{"lap": 3, "mark": "s1", "game_ms": 28411, "since_ms": 12, "attempt": 1, "prev": "<the receipt of lap 3's start>"}
```

`prev` is the receipt of the same lap's previous line mark, and is left out at `start`. A mark goes only once that receipt is in hand. The server answers with a receipt, `{"receipt": "tk1...."}`, which the launcher keeps as it is under `timetrial/ticks/` in its data folder. When a lap is collected into the outbox it takes the receipts of its `lap_index` with it, and its upload meta carries them in mark order:

```json
"timing": {"lap": 3, "receipts": ["tk1....", "tk1....", "tk1....", "tk1...."]}
```

A lap's pauses are marked too, up to four: `p1` as the game pauses, chained to the lap's start, and `r1` as it resumes, chained to `p1`, then `p2` and `r2`, up to `p4` and `r4`. `game_ms` is the game's lap clock, which stands still while it is paused. The server leaves the pause out of the lap's real time. A pause mark the server refuses, or one with no receipt of its chain, is dropped alone: that pause stays in the lap's time and the lap's other receipts and timing stay. A core before 1.10.0 refuses the pause marks.

| Answer | The launcher |
|---|---|
| no answer, 408 or a server error | sends the mark again after 0.5, 1, 2, 4, 8 and 16 s, then every 30 s, with `attempt` and `since_ms` updated |
| 429 | sends it again after the server's `Retry-After` |
| 400, 409 or another refusal | drops that lap's timing |
| 404 or 401 | stops ticking for the session |

A mark the server can't be reached for is kept for 10 minutes; one the server answered with an error is tried until a minute after its lap's finish. Then that lap's timing is dropped. A lap without receipts uploads without `timing`, as do laps from launchers or plugins without timing marks. The launcher sends at most 12 ticks a minute. A new lap waits up to 5 seconds for marks still being sent. The run log says when a mark couldn't be sent:

```
timing: lap 3 s1 not sent (couldn't reach https://...); trying again
timing: lap 3 end not sent within 600 s; that lap is untimed
```

## Session challenge

The launcher asks for the session nonce with the sha256 of the game's exe, `{"game_build": "<sha256>"}`, when it knows it. When Swofty's server sends a `challenge` with the nonce, the launcher writes it into `swofty/tt.json` as `tt_challenge`, next to `tt_nonce`, for the integrity module. A challenge that is not a JSON object or is larger than 16 KB is left out. Older servers send none, and the key is then absent.

## In-game boards

The Time Trial plugin shows addon content on the game's own Time Trial leaderboard screen, the Time Trial track select's View Leaderboards. It is off until the launcher writes `"tt_bridge": true` into `swofty/tt.json`, which it does when Time Trial boards in the game is on. Each step logs one `[boards]` line.

The game names a board `LBV` + the descriptor id (2 digits) + the track (2 digits) + `D` or `W`: `LBV0605D` is descriptor 6, track 5, dry. Descriptors 0 to 5 are the game's own. Descriptors 6 and up are addon boards.

A request for an addon board or a custom track's board goes to the launcher instead of the game's servers. The plugin writes it to `tt/outbox/<seq>.json` in the launcher's data folder (`swofty/tt/outbox/` in the game folder when it can't write the data folder):

```json
{ "seq": 12, "method": "GetTTLeaderboard",
  "args": { "LeaderboardID": "LBV0605D", "Version": 3, "RangeType": 0, "FromIndex": 0, "ToIndex": 9, "TopResultCount": 3, "CrossPlay": 1 },
  "board": { "content": "swofty.swofty-2022/f1_2022", "track": "bahrain", "weather": "dry", "game_board": "LBV0605D" },
  "player": { "ghost_id": "<base64 of the 0x30-byte id block, once a personal best was committed>" } }
```

The launcher answers with `tt/inbox/<seq>.json` beside it, `{ "seq": 12, "status": 200, "body": "<JSON text>" }`. Both sides write a `.tmp` file and rename it, and both ignore files older than 60 seconds. A request with no answer after 8 seconds completes with status 504 and the body `{"Message":"The request timed out","Data":null}`. Requests for the game's own boards go to the game's servers unchanged.

Guards are off. `"tt_guards": true` in `swofty/tt.json` or `SWOFTY_TT_GUARDS=1` turns them on. A lap in addon content or on a custom track then does not reach the publisher's boards.

## Barred addons

The boards server rejects a lap recorded with a barred addon enabled. It lists them, without a sign-in:

```
GET /v1/tt/barred
{ "addons": [
  { "id": "kizikafuka.ai-drive", "name": "AI Drive", "reason": "the AI can drive your car" },
  { "id": "swofty.rich-fuel-mix", "name": "Rich Fuel Mix", "reason": "...", "below": "1.0.3" }
] }
```

An entry with `below` bars the installed versions lower than it. An entry without `below` bars every version. An installed version that is not a version is barred.

The launcher keeps the list in `$DATA/cache/tt-barred.json` and asks for it again once it is five minutes old. A launch does not wait for the server: the wrapper writes the profile's enabled barred addons from that file into `swofty/tt.json`, then asks the server on a thread of its own and writes the key again when the answer differs. With nothing cached and the server unreachable, the key is left out.

```json
{ "tt_boards": true, "tt_barred": [ { "id": "kizikafuka.ai-drive", "name": "AI Drive" } ] }
```

While the game is in Time Trial, in the menus and on track, the core paints one red line at the top of the screen. The main menu and every other mode hide it. The core reads `tt.json` again every two seconds while in Time Trial. `SWOFTY_TT_BARRED=0` turns the line off.

```
[tt-barred] Not allowed in Time Trial, laps won't count: AI Drive
[tt-barred] the present hook arms for the Time Trial barred addons warning
[tt-barred] warning shown: the game is in Time Trial
[tt-barred] warning hidden
```

The launcher's Time Trial page shows the same addons with the reason, and `swofty laps list` prints them:

```
Not allowed in Time Trial, laps won't count: AI Drive
  AI Drive (kizikafuka.ai-drive): the AI can drive your car
```

## Events

`laps.recorded` fires on the flow thread once the file is in place. `laps.sealing` fires just before, on the core's writer thread, with the file as built so far; a handler reads the body it is given and does not read game memory.

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
[laps] wrote 20261110T041502Z-4.swlap lap 92011 ms (game), 5510 samples, ghost 0 bytes, board game/f1_2025|swofty.tracks:ring|dry, custom track (record from swofty.tracks)
```

`[laps] timing started` carries the same board and suffix.

The hooks log as they read:

```
[laps] lap result from the game: car 0 record 3 (message lap 3) 91234 ms, sectors 29001 31002 31231 ms, lap valid, sectors 111 valid
[laps] game lap extras: assists brk 1 abs 1 trc 2 drl 0 gbx 3 ers 1, input wheel, performance realistic
[laps] ghost blob from the game: 51284 bytes
```
