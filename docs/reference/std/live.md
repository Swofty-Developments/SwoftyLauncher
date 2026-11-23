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
- **Table:** the core export `SwoftyCoreLive` (`SwoftyLive`, table version 6 appends `car_detail`; version 5 appends `tyre_icon_texture`; version 4 appends image access; version 2 adds `driver_name` and version 1 ends at `seq`; version 3 adds `team_badge_texture`); `SWOFTY_API_VERSION` is unchanged
- **Feature:** `telemetry`, experimental, off by default (the live state is the memory source of std telemetry)
- **Thread:** any thread; car/session calls copy the core's per-frame snapshot. Badge/name getters read the game's memory at the call; `tyre_icon_texture` resolves the snapshot's fitted visual compound through the loaded GameDB.

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
[live] lap times off on the EA build: its layout is not proven there (SWOFTY_LIVE_FORCE=1 reads it anyway)
[live] session time left off: needs a session clock row read without a call (Sess_RaceRuleset_GetTimeRemaining interpolates RaceRuleset +0x2f0 from the game timer)
[live] off: build 1234567890 has no proven live layouts
```

Steam build **26566308** (F1 25 1.0.145.2472, executable PE timestamp `1794312636`) has a separate build descriptor for all existing Live State groups. Its ERS mode displacement is selected independently of the older Steam/EA layouts. Normal binding, ownership and validity checks still apply; values may be unavailable during loading or when game objects are absent. This preserves Live table v2 and the existing SDK/API. See `re/research/live-steam-26566308.md` for evidence and pending runtime validation.

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
    uint8_t garage;                // pit lane garage index, 99 the safety car (SWOFTY_LIVE_HAVE_GARAGE)
    uint8_t pit_box;               // garage / 2, 12 for the safety car (SWOFTY_LIVE_HAVE_GARAGE)
    uint8_t tyre_damage[4];        // RL RR FL FR: 0 none, 2 punctured, 3 burst (SWOFTY_LIVE_HAVE_TYRE_DAMAGE)
    float tyre_deflation[4];       // RL RR FL FR: 0, 1 when a puncture starts, 2 when the tyre bursts
    uint32_t reserved[1];
} SwoftyLiveCar;
```

| Call | Answers |
|---|---|
| `live::car(vehicle)` | one car by vehicle id |
| `live::subject()` | the watched car while the spectator HUD is up, else the player's |
| `live::player()`, `live::watched()` | from `session().player` and `.watched` |
| `live::turn(car)` | the official turn near `car.lap_m` |

`speed_kmh`, `ers_fraction`, `gap_ahead_ms`, `gap_leader_ms`, `in_pit`, `pit_status` and `pit_stops` return `std::optional` and are empty when the value is not there.

`garage` and `pit_box` take the first word of what was `reserved[7]` in earlier releases. `SwoftyLiveCar` keeps its 208 bytes and every other offset. They are 0, with `SWOFTY_LIVE_HAVE_GARAGE` clear, on a build whose live state does not walk the pit lane or whose game header does not export `PitLaneVehicle.garage_index`. Read them through [`swofty/systems/pitlane.hpp`](./pitlane).

`tyre_damage` and `tyre_deflation` take the next five words. They are 0, with `SWOFTY_LIVE_HAVE_TYRE_DAMAGE` clear, while the tyres group is off, on a build without the wheel puncture proof (the KB build and Steam 1.0.145 have it), or on a game header that does not export `CarWheel.tyre_flags` and `CarWheel.deflation`. Read them through [`swofty/systems/tyre_damage.hpp`](./tyre-damage).

## car_detail

```cpp
auto d = live::car_detail(7);
if (d && live::has(*d, SWOFTY_LIVE_HAVE_LAP_TIMES)) {
    uint32_t running = d->current_lap_ms;
    if (auto best = live::best_lap_ms(*d))
        swofty::log("[my-osd] best %u.%03u on lap %u", *best / 1000, *best % 1000, d->best_lap_number);
}
if (d && live::has(*d, SWOFTY_LIVE_HAVE_TYRE_WEAR))
    swofty::log("[my-osd] front left %.0f%%, %u laps on the set", d->tyre_wear[2], d->tyre_age_laps);
if (auto slot = d ? live::grid_position(*d) : std::nullopt)
    swofty::log("[my-osd] started P%u", *slot);
```

```c
typedef struct SwoftyLiveCarDetail {
    uint32_t version, size;
    uint64_t have;                 // the car's SWOFTY_LIVE_HAVE_* bits, as in SwoftyLiveCar.have
    uint32_t vehicle;
    uint8_t grid_position;         // starting grid slot from 1 (SWOFTY_LIVE_HAVE_GRID)
    uint8_t tyre_age_laps;         // laps on the fitted set, earlier stints on it included (SWOFTY_LIVE_HAVE_TYRE_AGE)
    uint8_t tyre_stint_laps;       // laps since the set was fitted (SWOFTY_LIVE_HAVE_TYRE_AGE)
    uint8_t lap_flags;             // SWOFTY_LIVE_LAP_CURRENT_VALID, SWOFTY_LIVE_LAP_LAST_VALID
    float tyre_wear[4];            // RL RR FL FR, percent: 0 new, 100 worn out (SWOFTY_LIVE_HAVE_TYRE_WEAR)
    uint32_t laps_completed;       // the rest: SWOFTY_LIVE_HAVE_LAP_TIMES
    uint32_t current_lap_ms;       // the lap in progress so far
    uint32_t current_sector_ms[2]; // its sector 1 and 2, 0 until the car crosses that line
    uint32_t last_lap_ms;          // 0 before the first completed lap
    uint32_t last_sector_ms[3];
    uint32_t best_lap_ms;          // fastest valid completed lap, 0 while there is none
    uint32_t best_lap_number;      // from 1
    uint32_t best_sector_ms[3];    // the best lap's sectors
    // from the game's UDP packets (see "From the game's UDP packets")
    uint8_t drs_allowed;           // 1 while the car may open DRS (SWOFTY_LIVE_HAVE_DRS_ALLOWED)
    uint8_t pit_limiter;           // 1 while the limiter is on (SWOFTY_LIVE_HAVE_PIT_LIMITER)
    int8_t fia_flag;               // -1 invalid, 0 none, 1 green, 2 blue, 3 yellow (SWOFTY_LIVE_HAVE_FIA_FLAG)
    uint8_t driver_status;         // 0 garage, 1 flying lap, 2 in lap, 3 out lap, 4 on track (SWOFTY_LIVE_HAVE_DRIVER_STATUS)
    float drs_distance_m;          // to the next activation point, 0 when none (SWOFTY_LIVE_HAVE_DRS_ALLOWED)
    uint8_t pit_lane_timer_active; // 1 while in the pit lane (SWOFTY_LIVE_HAVE_PIT_TIMING)
    uint8_t race_number;           // SWOFTY_LIVE_HAVE_IDENTITY
    uint16_t team_id;              // the UDP team id (SWOFTY_LIVE_HAVE_IDENTITY)
    uint32_t pit_lane_ms;          // in the pit lane, this or the last visit (SWOFTY_LIVE_HAVE_PIT_TIMING)
    uint32_t pit_stop_ms;          // stationary in the box, this or the last visit (SWOFTY_LIVE_HAVE_PIT_TIMING)
    uint32_t team_colour;          // 0xRRGGBB, the livery's first colour; 0 when the game sends none (SWOFTY_LIVE_HAVE_IDENTITY)
    uint16_t driver_id;            // the UDP driver id, before the core's stock id rewrite (SWOFTY_LIVE_HAVE_IDENTITY)
    uint8_t pad[2];
    uint32_t personal_best_sector_ms[3]; // the car's best time in each sector this session, 0 for none (SWOFTY_LIVE_HAVE_SECTOR_BESTS)
    float safety_car_delta_s;      // delta to the safety car's required time, s, as the game sends it (SWOFTY_LIVE_HAVE_SC_DELTA)
    uint32_t reserved[6];
} SwoftyLiveCarDetail;
```

| Call | Answers |
|---|---|
| `live::car_detail(vehicle)` | the car's detail from the same snapshot as `live::car`; empty on a core older than table version 6 |
| `live::grid_position(d)`, `live::last_lap_ms(d)`, `live::best_lap_ms(d)` | `std::optional`, empty when the bit is clear or the lap time is 0 |
| `live::drs_allowed(d)`, `live::pit_limiter(d)`, `live::fia_flag(d)`, `live::driver_status(d)` | `std::optional`, empty when the bit is clear; `driver_status` is a `live::DriverStatus` |
| `live::race_number(d)`, `live::team_colour(d)` | `std::optional`, empty when the identity bit is clear (`team_colour` also when it is 0) |
| `live::sector_bests(d)` | `std::array<uint32_t, 3>` of the personal best sectors, empty when the bit is clear |
| `live::safety_car_delta_s(d)` | `std::optional<float>`, the car's safety car delta, empty when the bit is clear |
| `live::driver_identity(d)` | `live::DriverIdentity{stat_id, code, first_name, last_name, label, name_reversed}`, see [Driver names](#driver-names) |

### Driver names

```cpp
if (auto who = live::driver_identity(*d))  // reads entrants.drivers: call it when Session::generation moves, not every frame
    swofty::log("[my-osd] %s %s (%s)", who->first_name.c_str(), who->last_name.c_str(), who->code.c_str());
// "Charles LECLERC (LEC)": the game's text for family names is upper case
```

`driver_identity` reads the `entrants.drivers` row whose `stat_id` is the car's `driver_id` with [swofty/db.hpp](./db): `code` is its `initials`, `first_name` and `last_name` the text of its `lng_*` keys in the game's language through [swofty/lang.hpp](./strings) (a stored string that is not a key reads as is, `default` as empty), `label` its `_metadata_` and `name_reversed` its `is_name_reversed` (the family name first). `live::driver_identity(uint32_t driver_id)` takes the id directly. It is empty without `SWOFTY_LIVE_HAVE_IDENTITY`, before the game has loaded its database, for driver id 255 (a network human, or an added driver with no stock ancestor), and when the id names more than one row: the 2024 and 2025 UDP formats carry one byte, so an added driver whose `stat_id` is above 254 shares its low byte with another row.

The values are the ones the game sends in its UDP output:

| Value | Game source |
|---|---|
| `grid_position` | `SessionVehicleEntry.grid_position_a ^ grid_position_b`, plus 1 |
| `tyre_wear` | 100 x `CarWheel.wear` of each wheel (the UDP car damage `tyresWear`) |
| `laps_completed`, lap and sector times | `VehicleLapTimesData`: record `current_lap` is the lap in progress, the record before it the last lap |
| best lap | of the records whose three sectors are set and whose valid pair holds, the first with the lowest time (`Sess_LapTimes_IsNewFastestLap`) |
| `tyre_age_laps` | the UDP car status `tyresAgeLaps`: the laps of every stint on the fitted set's compound and set index |

Lap times read every 0.1 s and the best lap every second; tyre wear and age every 0.5 s.

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
    // from the game's UDP packets (see "From the game's UDP packets")
    float time_left_s;             // SWOFTY_LIVE_SHAVE_TIME_LEFT
    uint32_t total_laps;           // SWOFTY_LIVE_SHAVE_TOTAL_LAPS
    float duration_s;              // the session's length, SWOFTY_LIVE_SHAVE_TIME_LEFT
    uint32_t fastest_lap_ms;       // SWOFTY_LIVE_SHAVE_FASTEST_LAP
    uint8_t fastest_lap_vehicle;   // vehicle id, SWOFTY_LIVE_SHAVE_FASTEST_LAP
    uint8_t safety_car_phase;      // SWOFTY_LIVE_SC_DEPLOYED, _RETURNING, _RETURNED, _RESUME (SWOFTY_LIVE_SHAVE_SAFETY_CAR_PHASE)
    uint8_t drs_enabled;           // 1 enabled, 0 disabled (SWOFTY_LIVE_SHAVE_DRS)
    uint8_t drs_reason;            // 0 wet, 1 safety car, 2 red flag, 3 minimum lap not reached, 0xff none
    uint8_t pit_window_ideal_lap;  // the player's, SWOFTY_LIVE_SHAVE_PIT_WINDOW
    uint8_t pit_window_latest_lap;
    uint8_t pit_rejoin_position;
    uint8_t start_lights;          // 1 to 5 lit, 0 from lights out (SWOFTY_LIVE_SHAVE_START_LIGHTS)
    uint8_t hud_visible;           // 1 while the game shows its race HUD (SWOFTY_LIVE_SHAVE_HUD)
    uint8_t hud_hidden_by;         // SWOFTY_LIVE_HUD_* bits: what hides it
    uint8_t pad2[2];
    uint32_t reserved[1];
} SwoftyLiveSession;
```

The fields from `time_left_s` on take what was `reserved[8]`; `SwoftyLiveSession` keeps its 440 bytes. `safety_car` reads 3 (formation) while the formation lap runs, from the UDP Session packet, where memory reads none.

```cpp
if (auto laps = live::total_laps(*s))
    swofty::log("[my-osd] lap %d of %u", leader.lap, *laps);
if (auto left = live::time_left_s(*s))
    swofty::log("[my-osd] %.0f s of %.0f left", *left, s->duration_s);
if (auto f = live::fastest_lap(*s))
    swofty::log("[my-osd] fastest lap: car %u, %u ms", f->vehicle, f->lap_ms);
```

| Call | Answers |
|---|---|
| `live::time_left_s(s)`, `live::total_laps(s)` | `std::optional`, empty when the bit is clear |
| `live::fastest_lap(s)` | `live::FastestLap{vehicle, lap_ms}`, empty when the bit is clear |
| `live::safety_car_phase(s)`, `live::drs_enabled(s)`, `live::start_lights(s)` | `std::optional`, empty until race control sends one this session |
| `live::pit_window(s)` | `live::PitWindow{ideal_lap, latest_lap, rejoin_position}` |
| `live::hud_visible(s)` | `std::optional<bool>`: whether the game shows its race HUD now; empty when the bit is clear |
| `live::hud_hidden_by(s)` | the `SWOFTY_LIVE_HUD_*` bits that hide it, 0 while it shows |

### The race HUD

An overlay that stands in for the stock HUD hides with it:

```cpp
void on_frame(double)
{
    auto s = live::session();
    if (!s || live::hud_visible(*s) == false)
        return; // menus, the track intro, the grid, cutscenes, replays, photo mode, the pause menu
    draw_my_hud(*s);
}
```

| `hud_hidden_by` bit | The game is in |
|---|---|
| `SWOFTY_LIVE_HUD_MENU` | no driving view: a menu, the track intro and its grid walk, the grid's Start Race screen, the results |
| `SWOFTY_LIVE_HUD_CUTSCENE` | a cutscene (the intro cameras, the formation lap cinematic, the winner and podium sequences) |
| `SWOFTY_LIVE_HUD_REPLAY` | an instant replay, a flashback's replay or the highlights |
| `SWOFTY_LIVE_HUD_PHOTO` | photo mode |
| `SWOFTY_LIVE_HUD_PAUSED` | a held pause reason (`swofty::view::pause_reasons()`), the pause menu among them |

`hud_visible` is 1 while driving (from the start lights on) and while spectating (the spectator HUD). The value comes from the game's flow states (the `[view] kind=… flags=…` log lines) and its pause calls, the same sources as [`swofty::view`](./events#pause-and-resume) and the post-processing view. At lights out the game passes through unnamed flow states for two or three frames between the start lights and the race; a menu reading that lasts 100 ms or less straight after the HUD showed keeps `hud_visible` at 1, so a menu that opens from driving reads hidden up to 100 ms late. The game's own HUD setting (HUD off in the options) is not read. `SWOFTY_LIVE_SHAVE_HUD` is clear until the core has seen a flow state, and `live::session()` itself is empty outside a session.

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

## Tyre images

`live::tyre_icon_texture(vehicle)` (Live table version 5) returns the fitted visual
compound's compact icon reference, for example
`#overrides.tyre_allocation.key_icons.Soft`. It resolves the effective GameDB
`texture_name` and the named `small` texture template, so a 2026 or modded visual
retains its own name. It does not infer the image from the physical compound or
`tyre_letter`. Missing/unverified fitted tyres, an unavailable GameDB, ambiguous
rows or an unsupported template answer empty. Older cores answer empty too.

Pass the reference to the same image service used for badges:

```cpp
if (auto reference = live::tyre_icon_texture(vehicle)) {
    if (auto images = live::images()) {
        uint8_t rgba[64 * 64 * 4];
        uint32_t width = 0, height = 0;
        uint64_t revision = 0;
        int status = images->image(reference->c_str(), rgba, sizeof rgba, &width, &height, &revision);
        // OK: copy/use the tight RGBA8 bytes; LATE: retry on a later update.
        // UNAVAILABLE: omit the image. Presentation and PNG encoding belong to the addon.
    }
}
```

The existing image table remains version 1. Live v5 adds support for the
`overrides.tyre_allocation.key_icons`, `compound_icons`, `tyre_icons` and
`overrides.tyres.tyres_small` catalogues. The default small icons are 28px;
larger arrays retain aspect ratio and are reduced to a maximum side of 64px.
The worker reads the effective UIDB slice names and array URI through active
archive redirects and resource overlays, preserving alpha. Route changes,
explicit clear and session retirement invalidate the cache and retire old work.
Differing duplicate archive resources answer unavailable rather than selecting
an arbitrary copy. In-place edits at unchanged routes need an explicit clear;
runtime-only GPU texture modifications are not captured for tyre icons.

## team_badge, team_badge_texture, driver_name and frame

```cpp
if (auto key = live::team_badge(car.vehicle)) // "ferrari_2026"
    texture = "#overrides.osd.timing_tower.team_badges_hud." + *key;

if (auto path = live::team_badge_texture(car.vehicle))
    texture = *path; // the path the stock timing pill binds for that car

if (auto name = live::driver_name(car.vehicle)) // "Charles Leclerc", UTF-8
    label = *name; // e.g. the text leaf of this standings row

auto f = live::frame(); // changes on every core refresh; poll it to skip unchanged frames
```

`team_badge_texture` answers one of:

```text
#overrides.osd.timing_tower.team_badges_hud.ferrari_2026            a catalogue team
eaid://ui_package/art/textures/render_badge_ePreLoaded 7             a team with show_custom_badge_in_UI: "<badge table> <badge index>"
eaid://ui_package/art/textures/render_badge_default.tga              a team with show_custom_badge_in_UI and no badge
```

The badge table name is `ePreLoaded`, `eUserDefined` or `eNetworkReplicated`. Both paths go into a `texture` binding as they are.

`team_badge` answers empty for My Team and for teams the game draws a rendered badge for. On the locally verified Steam 26566308 build, `team_badge_texture` resolves a My Team career badge from the current mode and loaded save slot in `playerCustomisationInfoSystem`'s saved assignments. It reads the checked SaveSlotGameModeComponent metadata and slot without invoking game functions. A missing slot, assignment or verified layout returns unavailable instead of using the entrant's potentially different fallback badge. Outside a My Team career, user-defined teams retain the entrant-badge path. The badge's own table and index select the resource independently of the team index. The rendered path reads in the group `rendered badges`, which is on for the Steam build. Older cores may return empty for My Team; cores older than table version 3 have no texture callback.

`driver_name` is the name the game itself shows for the car (`Sess_Get_CarDriverName`): the player's name for a car a human drives, else the display name of the driver record the car's session entrant points at, so a tower with one text leaf per row can fill each row from the classification order instead of carrying a copy of every car. The C call `driver_name(vehicle, out, cap)` writes at most `cap - 1` bytes, cut at a UTF-8 character boundary, then a NUL; it answers `SWOFTY_ASSET_INVALID` for a null buffer, `cap < 2` or a vehicle id of 32 or more, and `SWOFTY_ASSET_UNAVAILABLE` for a car the game has no name for. The C++ wrapper asks with a 128-byte buffer and answers empty on a core whose table is older than version 2. It reads the driver names group, which is off on the EA App build until its layout is compared there.

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
fake.set_driver_name(1, "Charles Leclerc");
fake.set_badge(1, "ferrari_2026");                 // team_badge and the catalogue team_badge_texture
fake.set_badge_texture(0, "eaid://ui_package/art/textures/render_badge_ePreLoaded 7");
fake.car(0).have |= SWOFTY_LIVE_HAVE_LAP_TIMES;
fake.detail(0).best_lap_ms = 90500;                // car_detail answers detail(0) with car(0).have
fake.install();

assert(swofty::live::gap_ahead_ms(*swofty::live::car(0)) == 950);
assert(swofty::live::order().ahead(0)->vehicle == 1);
assert(swofty::live::best_lap_ms(*swofty::live::car_detail(0)) == 90500u);
```

## When a group reads

The core reads each group (car systems, ERS, progress, classification, timing, pit, pit lane, penalties, tyres, flags, controllers, spectator, game mode, team badges, rendered badges, driver names, track, grid, lap times, tyre wear, tyre age and the game's message bus) only when every site it needs resolves on the running build and its KB rows are verified. The two tests are the same one: the generated bindings export a site only for a row that is L2 or better with an independent `agree`, and the generated game headers declare a struct field under the same rule, so a group turns on by itself when its rows land and the headers are regenerated. The offsets the reader uses are tied to the generated structs with `static_assert`s, so a header that declares a field somewhere else fails the build.

A group also needs its layout proven for the running build. The Steam build is the one the knowledge base describes. The EA App build (PE timestamp 1789128892) runs every group but driver names and rendered badges, because each struct field the groups read was compared against the Steam code that reads it, and every site they need resolves in the EA bindings set. One displacement differs and the core applies it for you: the game manager's progress system is at `+0x2a0` on EA and at `+0x2d0` on Steam, so the progress group (and everything the core derives from lap distance) reads that field at the right place on each build. A build in neither column, such as a future patch, runs no group at all.

| group | Steam | EA App |
|---|---|---|
| car systems, ERS, world position, DRS bit | yes | yes |
| progress | yes | yes, with the progress system at `+0x2a0` |
| classification, timing, pit, pit lane, penalties, flags, controllers | yes | yes |
| spectator, spectator gap, track, track name | yes | yes |
| tyres, game mode, team badges | yes | yes |
| bus events (collision, retirement) | yes | yes |
| driver names | yes | no, layout not compared yet |
| rendered badges | yes | no |
| grid, lap times, tyre wear, tyre age | yes | no, layout not compared yet |

Grid, lap times, tyre wear and tyre age read on the KB Steam build and on Steam 26566308; both EA App builds (the older one and 1.0.145) leave them off. Tyre wear also needs the wheel chain proof that tyre damage reads through.

A group that is yes still waits for its rows, as above, and the EA proof is static: it compares code, so run an EA race with `SWOFTY_TELEMETRY_VERIFY=1` once to compare the values with the game's UDP output. A group with any site that does not resolve stays off even with `SWOFTY_LIVE_FORCE=1`, which reads the groups whose sites resolve without waiting for verification, and on EA also without waiting for the layout proof; use it on a test machine to produce the evidence that verifies them, together with `SWOFTY_TELEMETRY_VERIFY=1` (see [Telemetry](./telemetry#memory-source)).

The player car comes from the controller system (the car whose active controller is the human one); `CarLive.is_ai` selects the physics rate and reads 1 for every car in a grid snapshot, so it is not used to find the player. With the controllers group off the player is unknown (`player` is -1) until it is.

A group whose chain the knowledge base has no verified row for reads only under `SWOFTY_LIVE_FORCE=1`.

The core also subscribes to the game's `r_car_to_car_col` and `vehicle_status_changed` bus messages (the bus group) to fire `telemetry.collision` and `telemetry.retirement` without UDP; see [Telemetry](./telemetry#events).

## From the game's UDP packets

The values below have no verified memory source. The core decodes them from the packets the game builds for its UDP telemetry, in the `sendto` hook the [telemetry](./telemetry) feed uses, and merges them into each snapshot; a field the memory read fills is never overwritten. They need the game's UDP telemetry output on (Settings, Telemetry Settings), at any port and send rate. The source is the same on the Steam and EA App builds. A packet value older than five seconds (fifteen for Participants) clears its bit; the time the game spends paused, when it sends no packets, does not count. The race control values hold until the session's UID changes.

| Field | Bit | UDP packet and field | 2024, 2025 | 2026 |
|---|---|---|---|---|
| `time_left_s`, `duration_s` | `SHAVE_TIME_LEFT` | Session (1) `m_sessionTimeLeft`, `m_sessionDuration` | yes | no |
| `total_laps` | `SHAVE_TOTAL_LAPS` | Session (1) `m_totalLaps` | yes | no |
| `pit_window_*`, `pit_rejoin_position` | `SHAVE_PIT_WINDOW` | Session (1) `m_pitStopWindowIdealLap`, `m_pitStopWindowLatestLap`, `m_pitStopRejoinPosition` | yes | no |
| `safety_car` 3 (formation) | `SHAVE_SAFETY_CAR` | Session (1) `m_safetyCarStatus` 3 | yes | no |
| `safety_car_phase` | `SHAVE_SAFETY_CAR_PHASE` | Event `SCAR` `eventType` | yes | yes |
| `drs_enabled`, `drs_reason` | `SHAVE_DRS` | Event `DRSE`, `DRSD` (`reason` in 2025) | yes | yes |
| `start_lights` | `SHAVE_START_LIGHTS` | Event `STLG` `numLights`, `LGOT` | yes | yes |
| `fastest_lap_*` | `SHAVE_FASTEST_LAP` | memory: the lowest `best_lap_ms` of every car while lap times read (Steam); else Event `FTLP` | yes | yes |
| `driver_status` | `HAVE_DRIVER_STATUS` | Lap Data (2) `m_driverStatus` | yes | yes |
| `pit_lane_*`, `pit_stop_ms` | `HAVE_PIT_TIMING` | Lap Data (2) `m_pitLaneTimerActive`, `m_pitLaneTimeInLaneInMS`, `m_pitStopTimerInMS` | yes | yes |
| `drs_allowed` | `HAVE_DRS_ALLOWED` | Car Status (7) `m_drsAllowed`, `m_drsActivationDistance` (distance 0 in 2026) | yes | yes |
| `fia_flag` | `HAVE_FIA_FLAG` | Car Status (7) `m_vehicleFiaFlags` | yes | yes |
| `pit_limiter` | `HAVE_PIT_LIMITER` | Car Status (7) `m_pitLimiterStatus` | yes | no |
| `race_number`, `team_id`, `driver_id` | `HAVE_IDENTITY` | Participants (4) `m_raceNumber`, `m_teamId`, `m_driverId` | yes | team and driver id only |
| `team_colour` | `HAVE_IDENTITY` | Participants (4) `m_liveryColours[0]` | 2025 only | no |
| `personal_best_sector_ms` | `HAVE_SECTOR_BESTS` | Session History (11) `m_bestSectorNLapNum` and that lap's sector time | yes | no |
| `safety_car_delta_s` | `HAVE_SC_DELTA` | Lap Data (2) `m_safetyCarDelta` | yes | yes |

`driver_id` is the `stat_id` of the car's `entrants.drivers` row for a stock driver; `live::driver_identity` reads its code and names ([Driver names](#driver-names)). The game sends 0 for both pit times once the car is out of the pit lane; the core holds the last visit's values until the next visit starts. `safety_car_delta_s` is the game's delta to the time the safety car or the virtual safety car requires; the game sends it in every Lap Data packet and it means something while `safety_car` reads 1 or 2. The session's best sector is the lowest `personal_best_sector_ms` over the cars. Session History carries one car per packet, so a car's bests refresh about once a second per car on the game's rotation.

The car index of a UDP packet is taken as the vehicle id, as for the telemetry feed (see [Telemetry](./telemetry#availability)). The race control messages (penalties, served penalties, DRS, start lights, the safety car phase and the fastest lap) fire as events; see [Race control](./race-control#race-control-messages). The 2026 rows read the 2025 entry offsets of the packets whose 2026 size the telemetry decoder knows; the 2026 Session and Participants layouts past the team id are not catalogued.

```text
[live] reading the session clock, lap count, race control messages, DRS, pit timing and identity from the game's UDP packets (format 2025)
```

## Not read yet

Active aero mode, Boost and Overtake state, car setup values and tyre temperatures have no verified memory source and no UDP source in the live state. `SWOFTY_LIVE_HAVE_BOOST` and `_AERO_MODE` are declared so the structs stay stable when they land and stay clear until then. The session time left, the race lap count, DRS allowed, the pit limiter and the identity fields read from UDP (above).

The 2026 values need the 2026 UDP layout, which the repository does not catalogue (`packages/core` has no 2026 game, and the decoders know the 2026 packet sizes only):

- **Session (1):** the 2026 size and every offset; until then the session clock, lap count, pit window and formation lap read in the 2024 and 2025 formats only.
- **Car Status (7):** the four bytes the 2026 entry adds (59 bytes, 55 in 2025) and the offsets of `activeAeroAvailable`, `activeAeroMode`, `activeAeroActivationDistance`, `activeAeroTrackStatus`, `overtakeAvailable`, `overtakeActive` and `overtakeActivationDistance`, for `HAVE_AERO_MODE` and `HAVE_BOOST`; the 2025 offsets of the pit limiter and the DRS distance are not proven in 2026.
- **Lap Data (2):** the 2026 delta to the best lap (`lapDeltaTime`).
- **Participants (4):** the 2026 offsets past `teamId` (race number, name, livery colours).
- **Session History (11) and Final Classification (8):** the 2026 sizes and layouts.

The game's 2026 packet builder (`Game_TelemetryOutputUDPUpdateable_PreparePackets`) writes every one of these. The core logs the unread session and setup values once:

```text
[live] session time left from memory off: needs a session clock row read without a call (Sess_RaceRuleset_GetTimeRemaining interpolates RaceRuleset +0x2f0 from the game timer); read from the UDP Session packet
[live] total laps from memory off: needs the race lap count: RaceRuleset +0x358 with the ruleset class proven on each session type; read from the UDP Session packet
[live] setup values off: needs the car setup record rows; CarOps gives several setup values
```

The setup values the car carries (brake bias, differential, suspension and the others in [Changing a car live](/std/systems/car-physics#changing-a-car-live)) read through the car operations instead. The game's own view-model leaves carry most of these values, but the core has no leaf reader: the node's value layout is not in the knowledge base, so there is no `hud::get` yet.

## Images through the live API (table v4)

`SwoftyCoreLive()->images()` (or `swofty::live::images()`) exposes pixels for the references returned by `team_badge_texture`. Check the outer table version before reading the appended member; older cores have no image access.

Call `start()` when enabling image access. Pass the exact reference to `image(reference, rgba, capacity, &width, &height, &revision)`. The caller owns the buffer (16,384 bytes covers the maximum 64 x 64 RGBA8 image); alpha and aspect ratio are preserved. `SWOFTY_ASSET_OK` returns pixels, `SWOFTY_ASSET_LATE` means an archive request is still loading, `SWOFTY_ASSET_UNAVAILABLE` means no usable pixels, and `SWOFTY_ASSET_INVALID` means invalid arguments or insufficient capacity. Retry on later frames without blocking the game thread.

Catalogue images follow the native router's current archive winners and accepted overlay copies. Runtime images use bounded, fenced D3D12 readback. The service writes no image files and returns no game-owned pointers. Session unload retires in-flight requests and archive results from the old session. Completed runtime captures may survive while their exact named GPU resources remain valid, bounded to 32 images; replacement resources, changed names/routes and explicit cache clearing invalidate them. Live session gates still clear the car references when the session ends. `sources()` is a routing receipt for diagnostics/invalidation, not a table-cell instruction; add-ons should leave global `clear()` to the core.

The add-on selects the car/reference, converts RGBA to an in-memory PNG if needed, and decides when to include it in a view. The launcher only renders the generic `image_data` cell. No launcher-side badge lookup, archive batch or column readiness state is involved.
