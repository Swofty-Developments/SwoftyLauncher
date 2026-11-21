---
title: Vehicle overrides
description: One team's seats drive another car for chosen events of a Grand Prix box or career season, as a team.vehicle_override record. Experimental, off unless SWOFTY_VEHICLE_OVERRIDE=1.
---

# Vehicle overrides

Give one team of a box another car for chosen events: the 1978 Brabham team drives the BT46B "fan car" at Anderstorp and the BT46 at every other round, or one Lotus seat drives the Lotus 88 for one weekend of 1981.

::: warning Experimental
`team.vehicle_override` is off unless the game starts with `SWOFTY_VEHICLE_OVERRIDE=1`. The swap is designed from the game's code only ([re/research/session_vehicle_swap.md](https://github.com/Swofty-Developments/SwoftyLauncher/blob/main/re/research/session_vehicle_swap.md)) and has not run in the game yet.
:::

A few terms used on this page:

- A **box** is one of the year tiles in Grand Prix. Its season build is a `content.package` record, and a career season races one.
- A **team entry** is one of the package's `teams`, keyed like `brabham`. Its `vehicle` is the team's own car, and its `drivers` are its seats in order.
- The **session copy** is the game's own copy of the entrant records for the session that loads. The game builds the session's cars from it at every session load.

## The header

```cpp
#include <swofty/systems/teams.hpp>

namespace swofty::teams {

struct VehicleOverrideWhen {                       // every field that is set must match the session
    Ref<tracks::Track> track;                      // empty: every track
    int year = 0;                                  // the box's season or the career's year; 0: every year
    std::vector<std::string> session_types;        // "practice", "qualifying", "sprint", "race"; empty: the whole weekend
};

struct VehicleOverride {                           // one package team's car for chosen events
    Ref<content::Package> package;                 // the box or career season whose team it changes
    std::string team;                              // the package's team entry key
    std::vector<Ref<drivers::Identity>> drivers;   // the seats it changes; empty: every seat of the team
    Ref<cars::Vehicle> vehicle;                    // a vehicle registered at boot
    Ref<liveries::Livery> livery;                  // empty: the vehicle's own livery
    VehicleOverrideWhen when;
};

inline Registry<VehicleOverride> vehicle_overrides{"team.vehicle_override", { .applies = Applies::session }};

}
```

## What the core does

- **At each session load** (Grand Prix and both careers) the core arms the records whose `package` is the active box's, or the career season's, and whose `when` matches the session's track, year and session type.
- **Before the game builds the session's cars** a wrap at the start of `Game_ChampionshipGameMode_BuildSessionSettings` writes the override's vehicle into the session copy of each matching seat: the vehicle ref (`+0x5c`) and no livery preset (`+0x64`, so the car wears its own livery). The game's function then runs unchanged, and the car's assets, AI data and handling follow the new vehicle.
- **An entrant belongs to the team** when its vehicle is the team entry's own car; a seat is its index in the team entry's `drivers`. Every fill of a session writes the same values.
- **Only the session copy changes.** The team, its name, colours, results and standings stay the team's, and the career's own rows and the box's entrant list keep the team's car.
- **The team's car goes back** at the session's end and before every career save (`career.saving`), and the override is written again after the save (`career.saved`), so a save never holds the override's vehicle. A record is put back only while it still holds what the core wrote.
- **Two overrides on one seat**: the record later in load order wins it, and the core logs both.

The core refuses:

- Time Trial (it fills its cars from whole lists of its own), every online mode and a shared session (a car only this game swaps would differ from the other players' cars), and the other game modes;
- a career while the save splice is not armed (`SWOFTY_SAVE_SPLICE=0`, or a build without it), since nothing would put the team's car back before a save;
- a record whose `vehicle` has no stat id (it is not a vehicle registered at boot: the game would drop the entrant), whose team entry is missing or has no car with a stat id, or whose `drivers` names a driver the team entry does not.

A `livery` other than empty is reported and the vehicle's own livery is used: 1.0 does not map a livery to the game's preset key yet.

Give the override's vehicle its own AI and handling data (`cars::Vehicle.ai`): the AI drives the participant's vehicle. Career R&D stays keyed by the team and the career's own car.

## The Brabham BT46B at Anderstorp 1978, as data

The season build names its package and its vehicles `f<yy>_<row>`, so the BT46B of the Brabham row is `vmknolle.season-1978/f78_<row>_bt46b`. Row `03` below stands for the Brabham row of the 1978 build. The BT46B is a vehicle of its own, registered at boot like every other car of the season (its `ai.asset_filename`, the package's `resource_path` and `ai_package_folder`), even though no entrant list uses it.

```json
{
  "team.vehicle_override": {
    "f78_brabham_bt46b_anderstorp": {
      "package": "vmknolle.season-1978/f1_1978",
      "team": "brabham",
      "drivers": [],
      "vehicle": "vmknolle.season-1978/f78_03_bt46b",
      "when": { "track": "vmknolle.tracks/anderstorp", "year": 1978, "session_types": [] }
    }
  }
}
```

Both Brabham seats drive the BT46B for the whole Anderstorp weekend, practice to race; every other round keeps the BT46. Leave `session_types` empty: a car that changes between two sessions of one weekend makes the game load the cars again for the session that changes it.

The same in code:

```cpp
#include <swofty/registry.hpp>
#include <swofty/systems/teams.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    teams::vehicle_overrides.add("f78_brabham_bt46b_anderstorp",
                                 { .package = content::packages["vmknolle.season-1978/f1_1978"],
                                   .team = "brabham",
                                   .vehicle = cars::vehicles["vmknolle.season-1978/f78_03_bt46b"],
                                   .when = { .track = tracks::tracks["vmknolle.tracks/anderstorp"], .year = 1978 } });
}

SWOFTY_ADDON()
```

## One driver: the Lotus 88

`drivers` with one entry changes only that seat; the team's other seat keeps the team's car. The 1969 four-wheel-drive cars (Lotus 63, McLaren M9A, Matra MS84) and the 1981 Lotus 88 are swaps like this.

```json
{
  "team.vehicle_override": {
    "f81_lotus_88_de_angelis_long_beach": {
      "package": "vmknolle.season-1981/f1_1981",
      "team": "lotus",
      "drivers": ["vmknolle.season-1981/de_angelis"],
      "vehicle": "vmknolle.season-1981/f81_11_lotus88",
      "when": { "track": "game/long_beach", "year": 1981, "session_types": [] }
    }
  }
}
```

A seat is the driver's place in the team entry's `drivers`, which is the entrant's team position in the session. A career driver who moved to that seat drives the override car too.

A third car for a team (an extra entrant for one event) is not an override: the override only changes the car of an existing seat.

## In the log

`loader.log` lines start with `[vehicle-override]`; the session load's `[apply]` lines name each armed record (stat ids and addresses are examples):

```
[vehicle-override] team.vehicle_override armed: a matching record swaps the seat's car in the session's entrant copy at each session load
[apply] team.vehicle_override session load: 1 applied, 0 put back, 0 refused
vmknolle.season-1978/f78_brabham_bt46b_anderstorp: armed: team brabham (car stat id 300), every seat -> vmknolle.season-1978/f78_03_bt46b (stat id 301)
[vehicle-override] vmknolle.season-1978/f78_brabham_bt46b_anderstorp: entrant at 0x...: vehicle 0x012c0000 -> 0x012d0000, livery key 0x00070000 -> 0x3fff0000
[vehicle-override] session settings (session id 15, 20 entrant(s), grand_prix race vmknolle.tracks/anderstorp): 2 written, 0 already written, 1 armed override(s)
[vehicle-override] 2 of 2 write(s) put back (the session ended)
```

## Readiness

| | |
|---|---|
| Status | experimental (1.10.15), off by default |
| Applies | at each session load (`Applies::session`) |
| Game code | `Game_ChampionshipGameMode_BuildSessionSettings` (L2 binding), the session's entrant records (`EntrantRecord` +0x5c, +0x64, +0x80) |
| Confirmed | static only: read in the game's code, not run in the game |
