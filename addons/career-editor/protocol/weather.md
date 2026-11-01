# Weather

Owner: `native/src/weather.cpp` (ce-weather), with the plan model (parser, state build, intensity clamp) in
`native/src/weather_plan.cpp`. Both modes. The ops only change the addon's sidecar
`weather_plan.json` (addon storage) and the weather datasets the game builds for the weekend, so nothing
is written to the save. The sidecar is keyed by `ce::career_id()` and then by weekend
(`{"version": 2, "careers": {"<career id>": {"seed", "plans", "locked"}}}`), so careers never share plans,
pins or locks. With no career id, every op fails with `not_ready`, and the hook applies no plan and no
seed pin (the game's own forecast plays). They are registered with `CE_OP_ANYTIME`, so they run whenever a career is loaded:
in Team HQ, and also in the weekend hub between sessions. A session is locked once it has started
(see `locked` below).

**How it applies.** The weekend hub generates the forecast for all eight weather datasets with
`GameWeatherSystem::setupSession` (wrapped: `Sess_GameWeatherSystem_SetupSession`). After the game has built
its own (realistic or custom) forecast, the wrap rebuilds every planned session's dataset through the
game's own dataset builder (`Sess_Tod_BuildSessionDataset_Thunk`, the call setupSession makes per session),
using the builder's exact-timing path, the one the game's scripted weather uses: one state per timeline
point, each state's hold and transition lengths given in seconds. Sessions that aren't in the plan keep the
game's forecast. The wrap fires on all three callers of setupSession:
- the weekend hub's forecast refresh;
- the GameWeatherSystem virtual at vtable slot +0x38, which sets ws+0x2f77 and then picks the current
  weather from the active dataset's clock (the session-time rebuild);
- the scripted-weather handler, which sets settings+0xb18. The wrap never touches it.

It also rebuilds once, immediately, when `weather.plan.set` runs while that weekend's forecast is live. A
time-trial build (settings+4 == 0x12) is left alone. The game's own session state (actual rain, wetness,
the forecast UI) then plays from that dataset.

The game makes P1–P3 a single state on a career's first weekend. A plan for those sessions replaces
that like any other forecast.

**Gates.** The build thunk does nothing while TOD+0xb432 is set or TOD+0xb429/+0xb42a is clear. With
TOD+0xb436 set, it diverts to a random build when TOD+0xb42d is set, or else to the scripted scenario
when TOD+0x2654 is set. A rebuild checks the same conditions first. When one fails, it logs
`weather: not_applied <key> (weekend <w>, <why>): <reason>` and reports the reason (op result
`after.not_applied`, snapshot `sessions.<key>.not_applied`) instead of claiming success.

**Sessions** (`session` keys; the game's weather datasets `eWDS*`):

| key | dataset | game sessions |
|---|---|---|
| `p1` | 0 Practice1 | P1, short practice |
| `p2` | 1 Practice2 | P2 |
| `p3` | 2 Practice3 | P3 |
| `q` | 3 Qualifying | Q1, Q2, Q3, short Q, one-shot Q |
| `ss` | 4 SprintShootout | SQ1, SQ2, SQ3, short SQ, one-shot SQ |
| `r` | 5 Race | Race (the sprint race on a sprint weekend, L0) |
| `r2` | 6 Race2 | Race 2 (the grand prix on a sprint weekend, L0) |
| `r3` | 7 Race3 | Race 3 |

The snapshot lists all eight with `active` (the game built a forecast with states). Which of `r`/`r2` is the
sprint needs in-game proof; the UI should label them from the season calendar's weekend format.

**States** (`state`): `clear`, `light_cloud`, `overcast`, `light_rain`, `heavy_rain`, `storm` (engine types
0..5; `storm` is normally produced only by the game's own heavy-rain variation, L1).

**Weather time.** Each dataset has a length in weather seconds, `length_s`: 3600 for a practice or
qualifying session, about 7200 for a full race, scaled by race length; for a 100% session it is the
session's real duration. Timeline positions become fractions of that length:
- `at_fraction`: 0..1 of the session.
- `at_minute`: minutes into the session. The fraction is `at_minute / minutes` when the session sets
  `minutes` (its real length in minutes), else `at_minute * 60 / length_s`.
- `at_lap`: needs `laps` on the session (the race's lap count; the launcher fills it from the season
  calendar). Lap N starts at fraction `(N - 1) / laps`. Lap 1 is the start of the race.

How the game advances weather time during a session (time-of-day clock × scale, L1) decides how close
"fraction of weather time" is to "fraction of laps". The progress log (below) measures it in game. Exact
laps under a safety car would need a live per-lap correction, which isn't done yet (phase 3).

### `weather.plan.set`
Args:
```json
{
  "weekend": 3,
  "replace": false,
  "sessions": {
    "r": {
      "laps": 57,
      "start": "overcast",
      "timeline": [
        { "at_lap": 15, "state": "light_rain", "intensity": 0.5 },
        { "at_lap": 22, "state": "heavy_rain", "intensity": 0.9, "ramp_s": 120 },
        { "at_lap": 40, "state": "overcast" }
      ],
      "air_temp": 18, "track_temp": 24
    },
    "q": { "timeline": [ { "at_fraction": 0, "state": "clear" } ] },
    "p1": null
  }
}
```
- `weekend` (int ≥ 0 or `"next"`, optional): the weather system's weekend index (`weather.weekend` in the
  snapshot, the value the game seeds weather with; L1, expected to be the calendar round index). If it's
  omitted, the op uses `weather.default_weekend`: the weather system's current weekend, or the one after it
  once that weekend's last race has started. When no weather system has been seen yet, it uses `"next"`,
  which binds to the first weekend whose forecast the hook builds.
- `replace` (bool, default false): true drops the weekend's stored sessions first. Otherwise sessions merge,
  and `null` removes one session.
- Per session:
  - `timeline`: 1..6 points (the game's limit is 6 states per session), each with exactly one of
    `at_fraction` / `at_minute` / `at_lap`, plus `state`. Optional `intensity` (0..1; omitted =
    the game's random draw within the type's range, see `types`; at each rebuild a planned intensity
    outside the type's live range is clamped into it, and one whose type has no usable range, not
    0 ≤ min ≤ max ≤ 1, is left to the game's draw) and `ramp_s` (transition length in weather
    seconds, 0..1800; default 2% of `length_s`, clamped to 30..300). The weather starts changing at the
    point's position and reaches the new state `ramp_s` later. Points are sorted by position; two points
    at the same position are an error.
  - Every transition the builder gets is at least 1 s long and every state after the first holds at least 1 s (a zero-length transition divides by zero in the game's blend). Points that would start inside the last second of the session are dropped, and the log says so (`weather: <key> (...): <n> point(s) dropped`).
  - `start` (state, optional): the state before the first point when the first point is after 0. If it's
    omitted, the session starts in the first point's state when that point is at 0, otherwise in the game's
    own first state for that session (`clear` if unknown).
  - `laps` (int 1..200): needed by `at_lap`. `minutes` (int 1..240): optional, see `at_minute`.
  - `air_temp` (-10..50), `track_temp` (-10..70), in °C: validated and stored, **not applied** (no game
    path for temperature is known yet; `temp_supported: false` in the snapshot).
  - `start` plus the timeline points can't exceed 6 states.
- Locked sessions: a session that has started this weekend (see snapshot `locked`) can't be changed or
  removed. The whole op fails with `session_locked` and names the sessions.

Result: `before`/`after` = `{ "weekend": int|"next", "sessions": { key: <stored session>|null } }` for the
touched sessions. `after.not_applied` is `{key: reason}` for sessions whose immediate rebuild didn't
happen or didn't read back. `after.applied_now` lists the sessions rebuilt immediately (only when `weekend` is the
live one). Undo: the journal's inverse is `weather.plan.set` with the `before` sessions (`null` where
none). Errors: `bad_args` (with the field), `session_locked`, `storage` (the sidecar write failed;
nothing changed).

Log: `[career-editor] weather.plan.set weekend=<w> <key>: before=<n|none> after=<n points|none>` per
session, plus one `[career-editor] weather: rebuilt <key> (weekend <w>, forecast build|plan.set)
states=<n> transitions=<n-1> len=<s>s: <type>(<intensity>)@<end>/<trans> ... <last type>(<intensity>)` line
for each rebuild, preceded by `[career-editor] weather: <key> (weekend <w>, <why>): state <i> <type>
intensity <a> -> <b> (game range <min>..<max>)` for each clamped intensity. The `rebuilt` line is the
dataset as the game holds it afterwards, read back. It ends in `MISMATCH` if the game's dataset doesn't
hold the planned states (then the per-state intensity isn't written and `rebuilt` stays false).

### `weather.plan.clear`
Args: `{ "weekend"?: int, "sessions"?: [key, ...], "started"?: bool }`. Removes the plan for those sessions
(all if `sessions` is omitted) of that weekend (default: the live one). Locked sessions are kept, and `kept`
lists them, unless `started` is true: then their plans are removed too. They stay locked, so
`weather.plan.set` still refuses them. Sessions already rebuilt this weekend, started or not, keep the
planned dataset until the next forecast build (the hub's refresh, or the session-time rebuild when a
session is loaded again), which then uses the game's own forecast. The launcher's Clear plan sends
`started: true`. Result `before`/`after` as for set. Undo:
`weather.plan.set` with `before`, which fails with `session_locked` for a started session.

### `weather.seed.lock`
Args: `{ "seed"?: u32 }`. Pins the realistic weather seed: the given seed, or the base seed the game is
using now (`weather.seed.base`). The wrap applies it before every forecast build. It sets the weather
system's forced-seed override (ws+0x1f14, which the game's `updateRandomSeed` honours), then runs the game's
own propagation, `WeatherAndTimeOfDayComponent::setWeatherSystemRandomSeed`
(`Sess_WeatherComponent_SetSystemRandomSeed`). That passes the component's seed and current weekend to
`updateRandomSeed`, which uses the forced value. It then sets ws+0x1f02/+0x1f04/+0x1f08 as the game
does, since setupSession's first-weekend rule reads them. The result: the realistic forecast is the one
the pinned seed produces for each weekend and session. If no WeatherAndTimeOfDay component owns the
weather system, the pin is stored but not pushed, and `after.not_applied` gives the reason. It's stored in the sidecar, re-applied every forecast build, and not written to the save (an
addon-less run uses the save's seed again). It takes effect at the next forecast build (entering or
refreshing the weekend hub). `"unsafe": true`: the forced-seed override is a raw runtime field, written
directly and then pushed through the game's own seed update; nothing caches it except the TOD seed, which
that update refreshes. Result: `before`/`after` = `{ "locked": bool, "seed": u32|null, "not_applied"?: string }`.
Undo: `weather.seed.unlock`, or lock with the previous seed.

### `weather.seed.reroll`
Args: `{ "seed"?: u32 }`. Like lock, with a new random seed (or the given one), so the next forecast build
produces a different realistic forecast. Planned sessions still use their plan. Result as for lock. Undo:
lock with `before.seed`, or unlock.

### `weather.seed.unlock`
Args: `{}`. Clears the pin and the forced override (only if it still holds our pin), then runs the game's
`setWeatherSystemRandomSeed`, which re-passes the component's own seed (`rnd_seed`). From the next forecast build the game uses its own seed again.
`"unsafe": true`, as for lock. Result as for lock.

Mode availability: all ops both modes. Errors common to all: `not_ready` (no career id, or for
`weather.seed.lock` without `seed`, no weather system seen yet), `bad_args`, `storage`.

### Snapshot section `weather`
```json
{
  "available": true,
  "career": "<career id>",
  "weekend": 3,
  "forecast_weekend": 3,
  "default_weekend": 3,
  "seed": { "base": 123456, "forced": 0, "tod": 123486, "locked": false, "pinned": null },
  "temp_supported": false,
  "active_dataset": 5,
  "types": [ { "state": "clear", "min": 0.0, "max": 0.0 }, { "state": "light_rain", "min": 0.2, "max": 0.5 } ],
  "sessions": {
    "r": {
      "dataset": 5, "active": true, "length_s": 7200.0, "progress": 0.0, "started": false, "locked": false,
      "planned": true, "rebuilt": true, "not_applied": null, "dynamic": false,
      "states": [ { "state": "overcast", "intensity": 0.3, "end": 0.26, "transition": 0.017 } ]
    }
  },
  "plan": { "weekend": 3, "sessions": { "r": { "laps": 57, "timeline": [ ... ] } } },
  "plans": [ 3, 4 ]
}
```
- `career`: the career id the plans belong to (`null` with no career; then `plan` is `null`).
- `weekend`: the weather system's current weekend index (`null` before a career or forecast is seen).
  `forecast_weekend`: the weekend the datasets below were built for. `default_weekend`: what an op without
  `weekend` targets.
- `sessions.<key>.states`: the dataset as the game holds it now (up to 38 states, since the game inserts
  in-between states for gradual changes). `end` is the fraction where the state's hold ends and the
  transition to the next one begins, and `transition` is that transition's length (both fractions of
  `length_s`). The last state is only the blend target of the one before it, so its `end` and
  `transition` are `null`. The game walks states − 1 transitions (WeatherDataset +0xdf8), while the full
  state count is at +0xdf0. A plan with N states reads back as N states (N = 1 reads back as 2 of the same
  type, from the builder's "not enough states" fill).
- `progress`: the fraction of weather time elapsed (L1; only meaningful for `active_dataset`).
  `started`: the session was entered (lifecycle `session.start` while this dataset was active), or it's the
  active dataset with progress > 0. `locked` = `started`. Locks persist in the sidecar per weekend, and are
  dropped for weekends after the one the hook sees (an earlier save loaded).
- `planned`: the stored plan has this session. `rebuilt`: the dataset was rebuilt from it this weekend and
  read back. `not_applied`: the reason the last rebuild didn't happen or didn't match (gate or MISMATCH),
  `null` otherwise.
- `dynamic`: the game marked the dataset as built by its realistic generator.
- `plan`: the stored plan for `weekend` (`null` if none). `plans`: every weekend that has a stored plan.
- `types`: the game's intensity range per state type (`min`/`max`), which the builder draws from.

### Logs for the in-game test
- `[career-editor] weather: hooks bound setupSession=… build=… setSystemSeed=…` once.
- `[career-editor] weather: seed pinned|released <seed> (<why>): base <a> -> <b>, component seed <c>, weekend
  <w>, tod seed <t>, first-weekend flags <f02>/<f04>/<f08>` on every pin push.
- `[career-editor] weather: forecast weekend=<w> mode=<m> exact=<e> seed=<base>/<forced>` on every forecast
  build, then `weather: game <key> states=<n> len=<s>s: …` per active dataset (what the game made).
- `[career-editor] weather: session.start dataset=<d> (<key>) weekend=<w> -> locked` when a session begins.
- `[career-editor] weather: progress <key> t=<clock>s scale=<x> ratio=<r> state=<i>/<n> <type>` every 30 s
  of real time while the active dataset's progress moves. That ties weather time to real time and laps:
  compare it with the lap count at those moments.
- `[career-editor] weather: sidecar loaded: …` / `no sidecar …` once, `weather: career <id>: <n> weekend
  plan(s), seed …` when the career changes, and `weather: events session.start=…
  on_frame=…` (both must be `ok` for locking and progress logs).

### Known risks (need in-game proof)
- The weekend hub has to rebuild the forecast (our wrap) after any frontend scene spawn. The frontend scene
  loader (0x1428c1ce0) rebuilds dataset 0 (`p1`) as plain clear for menu backgrounds, which affects the
  game's own P1 forecast the same way.
- `r` vs `r2` on sprint weekends, and whether `weather.weekend` equals the calendar round.
- Inside the hook (and `session.start`), an empty `ce::career_id()` keeps the last known career, so the
  session-time rebuild still applies the plan. Ops never do this: they refuse with `not_ready`. If no career
  has been seen yet this run, `weather: no career id; plans and the seed pin are not applied` is logged and
  the game's forecast plays.
- The session-time rebuild (vtable slot +0x38) should re-apply the plan before the session picks its
  current weather. Its `weather: rebuilt` line should come just before `session.start`.

### Bindings
All are KB rows (L2, dev-exported until a verifier agrees), and every address resolves by signature at
addon init.

| binding | kind | role here |
|---|---|---|
| `Sess_GameWeatherSystem_SetupSession` 0x1438b7350 | func (wrap) | forecast build; the plan is rebuilt after it returns, the seed pin is pushed before it |
| `Sess_Tod_BuildSessionDataset_Thunk` 0x14479c710 | func (call) | rebuilds one dataset with exact timing |
| `Sess_WeatherComponent_SetSystemRandomSeed` 0x140bde3e0 | func (call) | the game's seed propagation (pin / unpin) |
| `Sess_GameWeatherSystem_UpdateRandomSeed` 0x1438c80e0 | func (documented only) | not called; reached through the row above |

| struct (`re/db/structs`, subsystem session) | fields | used for |
|---|---|---|
| `GameWeatherSystem` | 9 | TOD and owner pointers, seeds, weekend, first-weekend flags, per-dataset realistic flag |
| `TimeOfDayOwner` | 1 | the build thunk's first argument, checked against the TOD |
| `TimeOfDaySystem` | 13 | clocks, active dataset, datasets, seed, dataset lengths, type ranges, exact/ready/busy flags, override gate (override_active, random_override, scripted_scenario) |
| `TimeOfDayClock` (0x288) | 4 | weather progress |
| `WeatherDataset` (0xe00) | 3 | states, full state count (+0xdf0), walked transitions (+0xdf8 = states − 1) |
| `WeatherState` (0x10) | 4 | type, intensity, end, transition |
| `WeatherTypeRange` (0x14) | 3 | per-type intensity range (planned intensities are clamped into it, and it's pinned while a planned intensity is built); the weight isn't read or written (only types 0..4 are weighted) |
| `WeatherSessionSettings` | 3 | session type (time trial skip), mode, exact flag (scripted skip) |
| `WeatherAndTimeOfDayComponent` | 2 | weather system pointer, `rnd_seed` |

## Plans follow their round
Each stored plan carries `event` (the round's weekend name) and `track` (its track stat id). After `add_round`,
`set_round`, `remove_round`, `reorder` and at every forecast build, a plan whose round moved is re-keyed to the round's new
index (`weather: plan for <track> moved with its round`), and one whose round is gone is dropped (`weather: plan for <track>
dropped`). A plan stored without `event` is stamped with the round at its index the first time it is seen. A session whose
plan names a wet state (light_rain, heavy_rain, storm) the track's per-type range cannot draw keeps the game's forecast
(`weather: <track> has no <state>; kept the game's forecast`).
