---
title: Seasons and GP/TT content
description: A whole Grand Prix and Time Trial season in code. Content packages and era boxes as typed registries, other addons building on a season in place or as a variant with its own box, each box's tracks and rules, stock files a year pack keeps, and the boxes a career's seasons race in.
---

# Seasons and GP/TT content

This page shows how a Grand Prix or Time Trial season is built, and how other addons build on it. A **box** is one of the year tiles a player picks in Grand Prix or Time Trial. In code, a box is a `content::EraBox` that opens a `content::Package`: the season's entrant list, with its teams, cars and drivers. Every piece is a record another addon can build on.

Classic Seasons 2017, for example, registers its season, its ten cars and its box. An engine sound pack then gives three of those cars a V10, and copies none of Classic Seasons' files.

## The header

<Example root="swofty-sdk" code-file="include/swofty/systems/content.hpp">
<template #code>

```cpp
#include <swofty/registry.hpp>

namespace swofty::content {

struct Seat {
    Ref<drivers::Identity> driver;                 // the driver
    int number = 0;                                // the car number this season
    Ref<drivers::Look> look;                       // the look this season (driver_customisation)
};

struct Team {
    std::string key;                               // the team's key in the package: "ferrari"
    Ref<teams::Identity> team;                     // the team record
    Ref<cars::Vehicle> vehicle;                    // its car
    Ref<sounds::EngineSound> engine_sound;         // the car's sound; empty: the vehicle's. Applied at boot, to that car
    Ref<liveries::Livery> livery;                  // the car's livery in this package; empty: the vehicle's
    std::vector<Seat> drivers;                     // its seats
};

struct Package {                                   // a GP/TT season build: an entrant list and its rows
    Text title;                                    // the season's name: a lang key ("lng_cs_f1_2017")
    std::string entrant_list;                      // "cs_f1_2017"
    int season = 0;                                // the year
    Ref<career::Calendar> calendar;                // the season's real calendar: first in the box's track list
    Keyed<Team, &Team::key> teams;                 // the grid, matched by key: p.teams.at("ferrari")
    sessions::Rules rules;                         // the box's points, DRS, tyre classes and rules, applied at session load
    std::string setup_mode = "swofty_f1";          // shared by every box; your own only for a different shape at boot
    StatId stat_id = StatId::automatic;            // the entrant list's id
};

struct TimeTrial {
    std::string entrant_list;                      // the list the box selects in Time Trial (default: the package's)
};

struct EraBox {                                    // an entry in the GP/TT era picker
    Ref<Package> package;                          // the package it opens
    Text title;                                    // its title (default: the package's)
    std::string image;                             // "#overrides.home_screen.tiles_solo_6x6[cs_f1_2017]"
    int order = 0;                                 // its place in the Addons row
    TimeTrial time_trial;                          // since 1.1: its Time Trial list
};

inline Registry<Package> packages{"content.package", { .applies = Applies::content }};   // its entrant list rows are added at boot
inline Registry<EraBox> era_boxes{"era.box", { .applies = Applies::read }};   // when the picker is built

}
```

</template>
</Example>

## Classic Seasons 2017, in code

<Split>

Classic Seasons 2017 registers its season as records, and the core applies them at boot.

- **The era box** is what the player picks. It points at a package.
- **The package** is the season build: the entrant list, the teams, their cars and drivers.
- **The vehicle** is one car, with its sound and livery ([Cars](/std/systems/cars)).
- **The engine sound** is a bundle, plus where its mixing settings and tuning come from ([Sounds](/std/systems/sounds)).

<template #example>
<Example root="swofty.classic-seasons-2017" code-file="native/src/content.cpp" data-file="registry/content.json" files="swofty.json, native/src/cars.cpp, native/src/calendar.cpp, sounds/cs17_ferrari.bdl, sounds/cs17_mercedes.bdl, sounds/cs17_honda.bdl, sounds/cs17_renault.bdl, lang/season.json">
<template #code>

```cpp
#include <swofty/registry.hpp>
#include <swofty/systems/career.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/content.hpp>
#include <swofty/systems/drivers.hpp>
#include <swofty/systems/liveries.hpp>
#include <swofty/systems/sounds.hpp>
#include <swofty/systems/teams.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    auto ferrari_sound = sounds::engine_sounds.add("ferrari", {
        .bundle = file("sounds/cs17_ferrari.bdl"),
        .settings_from = sounds::engine_sounds["game/ferrari_2025"],
        .tuning_from = sounds::engine_sounds["game/ferrari_2025"],
    });

    auto ferrari = cars::vehicles.variant("ferrari", cars::vehicles["game/ferrari_2025"], [=](cars::Vehicle &v) {
        v.label = "cs17_ferrari";
        v.engine_sound = ferrari_sound;
        v.livery = liveries::liveries["swofty.classic-seasons-2017/ferrari"];
    });

    auto season = content::packages.add("f1_2017", {
        .title = "lng_cs_f1_2017",
        .entrant_list = "cs_f1_2017",
        .season = 2017,
        .calendar = career::calendars["swofty.classic-seasons-2017/f1_2017"].optional(),   // the real 2017 calendar (calendar.cpp)
        .teams = {
            { .key = "ferrari", .team = teams::identities["swofty.classic-seasons-2017/ferrari"], .vehicle = ferrari,
              .drivers = { { .driver = drivers::identities["game/sebastian_vettel"], .number = 5 },
                           { .driver = drivers::identities["game/kimi_raikkonen"],   .number = 7 } } },
            // mercedes, redbull, force_india, williams, mclaren, toro_rosso, haas, lotus, sauber
        },
    });

    content::era_boxes.add("f1_2017", {
        .package = season,
        .image = "#overrides.home_screen.tiles_solo_6x6[cs_f1_2017]",
        .order = 2017,
    });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
// registry/content.json
{
  "engine_sound": {
    "ferrari": { "bundle": "sounds/cs17_ferrari.bdl", "settings_from": "game/ferrari_2025", "tuning_from": "game/ferrari_2025" }
  },
  "vehicle": {
    "ferrari": { "extends": "game/ferrari_2025", "label": "cs17_ferrari",
                 "engine_sound": "swofty.classic-seasons-2017/ferrari", "livery": "swofty.classic-seasons-2017/ferrari" }
  },
  "content.package": {
    "f1_2017": {
      "title": "lng_cs_f1_2017", "entrant_list": "cs_f1_2017", "season": 2017,
      "calendar": "swofty.classic-seasons-2017/f1_2017?",
      "teams": [
        { "key": "ferrari", "team": "swofty.classic-seasons-2017/ferrari", "vehicle": "swofty.classic-seasons-2017/ferrari",
          "drivers": [ { "driver": "game/sebastian_vettel", "number": 5 }, { "driver": "game/kimi_raikkonen", "number": 7 } ] }
      ]
    }
  },
  "era.box": {
    "f1_2017": { "package": "swofty.classic-seasons-2017/f1_2017",
                 "image": "#overrides.home_screen.tiles_solo_6x6[cs_f1_2017]", "order": 2017 }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] register: swofty.classic-seasons-2017 (classic_seasons_2017.dll) 4 engine_sound, 10 vehicle, 10 vehicle.package,
           10 team.identity, 1 content.package, 1 era.box (early stage, 1.8 ms)
[registry] applied at boot: engine_sound 4 (4 bundle paths redirected to swofty.classic-seasons-2017), vehicle 10,
           vehicle.package 10, team.identity 10, content.package 1 (entrant list cs_f1_2017, 20 entrants; 1,912 database records)
[era]      era.box swofty.classic-seasons-2017/f1_2017: Addons row, order 2017
```

## The V10 pack builds on it, in place

<Split>

An engine sound pack brings its own sound under its own key, then gives it to three 2017 teams with a layer on the 2017 package. In place, every user of the 2017 package hears it: the 2017 GP/TT box, Progressive Career's 2017 season, anything else that races those cars.

Classic Seasons doesn't know the pack exists, and the pack copies none of Classic Seasons' files.

A car's sound is a database row the game reads at boot. The core applies a team's `engine_sound` at boot, to the car that team drives, and the car has that sound wherever it races. Whether the game reads sound rows again later needs reverse engineering.

<template #example>
<Example root="mod.esp" code-file="native/src/esp.cpp" data-file="registry/layers.json" files="swofty.json, sounds/v10_screamer.bdl, registry/sounds.json">
<template #code>

```cpp
#include <swofty/registry.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/content.hpp>
#include <swofty/systems/sounds.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    auto v10 = sounds::engine_sounds.add("v10_screamer", {
        .bundle = file("sounds/v10_screamer.bdl"),
        .settings_from = sounds::engine_sounds["game/ferrari_2025"],
        .tuning_from = sounds::engine_sounds["game/ferrari_2025"],
    });

    content::packages.layer(content::packages["swofty.classic-seasons-2017/f1_2017"], [=](content::Package &p) {
        for (const char *team : { "mercedes", "ferrari", "redbull" })
            p.teams.at(team).engine_sound = v10;
    });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
// registry/layers.json (the engine sound itself is in registry/sounds.json)
{
  "content.package": {
    "swofty.classic-seasons-2017/f1_2017": {
      "teams": [
        { "key": "mercedes", "engine_sound": "mod.esp/v10_screamer" },
        { "key": "ferrari",  "engine_sound": "mod.esp/v10_screamer" },
        { "key": "redbull",  "engine_sound": "mod.esp/v10_screamer" }
      ]
    }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] register: mod.esp (esp.dll) 1 engine_sound, 1 layer on content.package (early stage, 0.4 ms)
[registry] content.package swofty.classic-seasons-2017/f1_2017: edit by mod.esp changed /teams/key=ferrari/engine_sound,
           /teams/key=mercedes/engine_sound, /teams/key=redbull/engine_sound -> mod.esp/v10_screamer
[registry] applied at boot: engine_sound 1 (1 bundle path redirected to mod.esp); content.package
           swofty.classic-seasons-2017/f1_2017 team sounds: 3 cars (cs17_mercedes, cs17_ferrari, cs17_redbull)
```

## Or as a variant, with its own box

<Split>

The pack can instead add a separate "F1 2017 (V10)" entry and leave the real one alone. The variant package starts from the 2017 package. Its three teams drive their own vehicle variants with the V10, and the 2017 box keeps its cars and their sounds. Its era box sits next to the original.

Every team, seat and field the variant doesn't change comes from the base, and later layers on the base flow in. The variant inherits the base package's [calendar and track data](#a-track-is-in-every-year).

The variant's era box shows like the base's box. A box that sets no `image` uses the tile art of the nearest base package's box. A literal `title` is shown as written. A variant vehicle that gets its own label keeps its base car's team row and side rows (names, colours, pit times), so its team shows under the base's name and stays a separate team in team select.

<template #example>
<Example root="mod.esp" code-file="native/src/esp_variant.cpp" data-file="registry/content.json" files="swofty.json, sounds/v10_screamer.bdl, registry/sounds.json, lang/esp.json">
<template #code>

```cpp
#include <map>
#include <string>

#include <swofty/registry.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/content.hpp>
#include <swofty/systems/sounds.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    auto v10 = sounds::engine_sounds.add("v10_screamer", { .bundle = file("sounds/v10_screamer.bdl"),
                                                            .settings_from = sounds::engine_sounds["game/ferrari_2025"] });

    // the box's own cars: "mod.esp/ferrari_v10" extends Classic Seasons' 2017 Ferrari
    std::map<std::string, Ref<cars::Vehicle>> v10_cars;
    for (std::string team : { "mercedes", "ferrari", "redbull" })
        v10_cars[team] = cars::vehicles.variant(team + "_v10", cars::vehicles["swofty.classic-seasons-2017/" + team],
            [=](cars::Vehicle &v) {
                v.label = "esp_cs17_" + team;
                v.engine_sound = v10;
            });

    auto base = content::packages["swofty.classic-seasons-2017/f1_2017"];
    auto season = content::packages.variant("f1_2017_v10", base, [=](content::Package &p) {
        p.title = "lng_esp_era_2017_v10";
        p.entrant_list = "esp_f1_2017_v10";
        for (const auto &[team, car] : v10_cars)
            p.teams.at(team).vehicle = car;
    });

    content::era_boxes.add("f1_2017_v10", { .package = season, .order = 2017 });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
// registry/content.json
{
  "vehicle": {
    "mercedes_v10": { "extends": "swofty.classic-seasons-2017/mercedes", "label": "esp_cs17_mercedes", "engine_sound": "mod.esp/v10_screamer" },
    "ferrari_v10":  { "extends": "swofty.classic-seasons-2017/ferrari",  "label": "esp_cs17_ferrari",  "engine_sound": "mod.esp/v10_screamer" },
    "redbull_v10":  { "extends": "swofty.classic-seasons-2017/redbull",  "label": "esp_cs17_redbull",  "engine_sound": "mod.esp/v10_screamer" }
  },
  "content.package": {
    "f1_2017_v10": {
      "extends": "swofty.classic-seasons-2017/f1_2017", "title": "lng_esp_era_2017_v10", "entrant_list": "esp_f1_2017_v10",
      "teams": [
        { "key": "mercedes", "vehicle": "mod.esp/mercedes_v10" },
        { "key": "ferrari",  "vehicle": "mod.esp/ferrari_v10" },
        { "key": "redbull",  "vehicle": "mod.esp/redbull_v10" }
      ]
    }
  },
  "era.box": { "f1_2017_v10": { "package": "mod.esp/f1_2017_v10", "order": 2017 } }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] vehicle mod.esp/ferrari_v10: extends swofty.classic-seasons-2017/ferrari; changed /label, /engine_sound
[registry] content.package mod.esp/f1_2017_v10: extends swofty.classic-seasons-2017/f1_2017; changed /title, /entrant_list,
           /teams/key=mercedes/vehicle, /teams/key=ferrari/vehicle, /teams/key=redbull/vehicle
[era]      era.box mod.esp/f1_2017_v10: Addons row, order 2017 (after swofty.classic-seasons-2017/f1_2017)
```

[Events](/std/events) scoped to the 2017 package also run in its variant, "F1 2017 (V10)".

## Keep a stock file

A year pack can pin a stock file with `stock("...")`. The game then reads its original file at that path, whatever an import or an older addon's overlay put there.

Classic Seasons 2024's Red Bull uses the stock Red Bull sound. An imported F1 2012 sound pack replaces the stock bundles with V8s. The 2024 pack pins the stock bundle, and its car keeps the 2024 sound:

```cpp
// swofty.classic-seasons-2024: native/src/sounds.cpp
auto redbull_sound = sounds::engine_sounds.add("redbull", {
    .bundle = stock("audio/2025/bundles/redbull_2025.bdl"),     // the game's original, whatever an import replaced
    .settings_from = sounds::engine_sounds["game/redbull_2025"],
});
```

```json
// registry/sounds.json, without code
{ "engine_sound": { "redbull": { "bundle": { "stock": "audio/2025/bundles/redbull_2025.bdl" },
                                 "settings_from": "game/redbull_2025" } } }
```

Every other box that uses the stock sound still hears the import.

## A track is in every year

<Split>

A track addon goes into the base that every box inherits. Hockenheim appears in the 2017 box too, with its base data.

The box's track list shows the season's real calendar first, in round order: the calendar the package names in `content::Package::calendar`. Every other track that fits the box follows under **More tracks**. A track fits a box when:

- The world the box loads at that track has a garage for every team in the box.
- The track has a grid slot for every car the box runs.
- Every car in the box has AI data at that track.
- In a Time Trial box, the track supports Time Trial.

A track that doesn't fit is left out of that box. The track select screen's list builder needs reverse engineering. Until it's understood, the screen shows every track in the base in the game's own order, and a guard stops a track that doesn't fit before the session loads.

Each box also has its own view of each track in `tracks::per_package`, keyed `<package>/<track>`: whether it's offered, its place under More tracks, and per-track data for that box only. An empty field comes from the base track data. In 1.6.5 the core applies a box's `fuel`; its other fields are stored. `fuel.burn_per_lap` is the track's fuel-per-lap figure for the MFD fuel delta, the fuel strategy and refuelling; the car's real burn comes from its power unit (see [What `burn_per_lap` changes](/std/systems/tracks#what-burn-per-lap-changes)).

Classic Seasons 2016 to 2024 each register the season's real calendar and name it in the package, optionally: a round at a venue no stock track provides, such as Sochi in 2017, is dropped until a track addon provides it (see [A calendar that uses it](/std/systems/tracks#a-calendar-that-uses-it-optionally)).

When the box is picked, or a career season that races in the box starts, the core applies its view to the track data the game reads. It puts the view back when that content ends. See [Tracks](/std/systems/tracks).

<template #example>
<Example root="swofty.classic-seasons-2017" code-file="native/src/tracks.cpp" data-file="registry/tracks.json" files="swofty.json, native/src/content.cpp, native/src/calendar.cpp">
<template #code>

```cpp
SWOFTY_REGISTER
{
    // the package names its calendar (content.cpp), and that calendar comes first in the box's list
    auto box = content::packages["swofty.classic-seasons-2017/f1_2017"];

    // 2017 fuel at two tracks; every other track keeps its base data
    tracks::per_package.add("f1_2017/monaco", { .package = box, .fuel = tracks::Fuel{ .burn_per_lap = 1.45f } });
    tracks::per_package.add("f1_2017/spa_francorchamps", { .package = box, .fuel = tracks::Fuel{ .burn_per_lap = 1.80f } });
}
```

</template>
<template #data>

```json
{
  "track.per_package": {
    "f1_2017/monaco":            { "package": "swofty.classic-seasons-2017/f1_2017", "fuel": { "burn_per_lap": 1.45 } },
    "f1_2017/spa_francorchamps": { "package": "swofty.classic-seasons-2017/f1_2017", "fuel": { "burn_per_lap": 1.80 } }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[tracks] box swofty.classic-seasons-2017/f1_2017 (grand_prix): calendar 18 of 20 rounds (venue/sochi: no track; venue/sepang: no track), more tracks 10
[registry] content swofty.classic-seasons-2017/f1_2017 active: 2 scoped changes applied (track data: monaco fuel 1.45, spa_francorchamps fuel 1.80)
[registry] content mod.esp/f1_2017_v10 active: calendar and track data inherited from swofty.classic-seasons-2017/f1_2017
[registry] content swofty.classic-seasons-2017/f1_2017 ended: 2 scoped changes put back
```

A box's list can also target a tag. A **tag** is a shared, named set of records, such as `c/street_circuits`, that any addon can add its tracks to. The F1 1990 box leaves every street circuit out of More tracks, including ones added later:

```cpp
auto street = tracks::tracks.tag("c/street_circuits");
tracks::per_package.layer(where<tracks::PackageTrack>([=](const tracks::PackageTrack &t) {
    return t.package.id() == "swofty.f1-1990/f1_1990" && street.contains(t.track);
}), [](tracks::PackageTrack &t) { t.offered = false; });
```

## Rules per box

Every Grand Prix and Time Trial box shares the `swofty_f1` setup mode. A **setup mode** is the game's rule set a weekend runs under, and the core narrows `swofty_f1` to the box the player pressed. Each box's points, DRS, tyre classes and rules (`content::Package::rules`) apply when a session loads in that box, and every other box keeps its own. See [Sessions](/std/systems/sessions). A box's DRS rule (`rules.drs`) applies from 1.6.4: [DRS per box](/std/systems/power-units#drs-per-box).

Only a box that needs a different shape at boot gets its own setup mode. F1 1990's box has its own mode for its two qualifying sessions.

Grid size is one boot setting per install, and each box is clamped to it.

## Time Trial

A box can select a different entrant list in Time Trial with `time_trial.entrant_list` (`era.box` 1.1). This 2012 box races 24 cars in a Grand Prix and 12 in Time Trial:

```json
// registry/content.json
{
  "content.package": {
    "f1_2012":    { "title": { "literal": "F1 2012" }, "entrant_list": "ex_f1_2012",    "season": 2012, "teams": [ ... ] },
    "f1_2012_tt": { "title": { "literal": "F1 2012" }, "entrant_list": "ex_f1_2012_tt", "season": 2012, "teams": [ ... ] }
  },
  "era.box": {
    "f1_2012": {
      "package": "mod.example/f1_2012",
      "order": 2012,
      "time_trial": { "entrant_list": "ex_f1_2012_tt" }
    }
  }
}
```

The same box from code, with `swofty::era::add_box` (`swofty/era.hpp`):

```cpp
swofty::era::add_box({
    .entrant_list = "ex_f1_2012",
    .title = "lng_ex_f1_2012",
    .order = 2012,
    .time_trial = "ex_f1_2012_tt",                 // empty: entrant_list
});
```

- **The car limit.** A Time Trial session holds at most **23** cars of the list it was picked with. The game adds three cars of its own to a table of 26. A 24-car list crashes Time Trial when the session loads.
- **On the Time Trial picker** the box selects its Time Trial list, and the core narrows `swofty_f1` to it. On the Grand Prix picker the box selects its package's list.
- **The list** is set up like the package's: a second package's list, or a list your database operations add to `swofty_f1`'s entrant lists.
- **No Time Trial list** means the package's list, under the same limit.
- **The check.** Each time the Time Trial picker is built, the core leaves a box off it, with one log line, when its list isn't offered under `swofty_f1` or holds more than 23 cars. At boot, the core's `era.box` report also names a box whose package has more than 23 cars and no Time Trial list.
- **The guard.** The core keeps a list of more than 23 cars out of every Time Trial session, whichever box or addon selected it:
  - A press on a stock box in Time Trial leaves `swofty_f1` offering only its lists of at most 23 cars until the next press, and no list at all when none of them fits. A list whose car count can't be read stays offered.
  - When a Time Trial session loads, the core logs the list the game resolved. When that list, or the first list of its entrant setup mode, holds more than 23 cars, the core loads a list of at most 23 cars from the same entrant setup mode instead. It tries the box pressed first, then that box's Time Trial list, then the first list that fits.
  - The session's entrant setup mode draws at most 23 cars (`max_entrants` above 23, or unset, is held at 23) until the session ends. With no list that fits, this keeps the session at 23 cars of the list.
  - A Time Trial takes more than its own list: one car for each distinct vehicle of every list on the selected list's leaderboard, in every setup mode the Time Trial offers, plus 4 entries of its own, in a table of 26. The stock F1 2025 box draws 11 cars and spawns 15 vehicles. A list copied from `f1_2025` keeps its leaderboard (`f1`), so F1 2012 Season's 12 cars made it 27; Classic Seasons' lists use leaderboard `classic`. Each Time Trial load counts those cars the way the game takes them: in the setup modes the Time Trial's override rows swap in (F1 2025's `f1` becomes `f1_custom_team`), each list only where its setup mode takes list cars, and the entrants system's own cars when the mode takes those. It leaves whole lists out, stock lists kept before addon lists, until at most 22 remain. The selected list always stays. A restart keeps the cut, and the held `max_entrants` and list swap; they come back at StateUnloadSession Exiting. With the core's 16 restore slots full, a cut is skipped and logged. Each session trace line in a Time Trial shows the budget's expected vehicles, and a separate line names any step where the fill made a different number. Before the fill (StateLoadSession Entering and Updating) a count is compared only when it changed since the step before, since the previous session's participants are still there.
  - Each session load logs a `session trace` line at StateLoadSession Entering, Updating and Exiting, at StateSession Entering and at the session's vehicle setup: the entrants with their team ids, the list, the setup mode, the session participants and the vehicles.
  - Everything the guard changed is put back when the session unloads (StateUnloadSession Exiting) or a box is pressed.
- A core before launcher 1.6.2 ignores `time_trial` and has no guard.

```text
[era]      box ex_f1_2012 from mod.example registered (title lng_ex_f1_2012, image #overrides.home_screen.tiles_solo_6x6[ex_f1_2012], order 2012, Time Trial list ex_f1_2012_tt): live from the next build of the picker
[era]      box ex_f1_2016 from mod.other skipped in Time Trial: its Time Trial list ex_f1_2016 holds 24 cars; Time Trial holds at most 23 (the game adds 3 cars of its own to a table of 26). Give the box a Time Trial entrant list of at most 23 cars
[era]      swofty_f1 keeps ex_f1_2016 out of Time Trial while a stock box is picked: it holds 24 cars; Time Trial holds at most 23 (the game adds 3 cars of its own to a table of 26)
[era]      swofty_f1 keeps bdx_s5_2012 out of Time Trial while a stock box is picked: it holds 24 cars; Time Trial holds at most 23 (the game adds 3 cars of its own to a table of 26); no list of it fits, so it offers none until the next press
[era]      the Time Trial session's list f1_custom_team (leaderboard 0, f1) draws 11 cars from 1 list(s) of its leaderboard; with the game's 4 entries 15 of the 26-slot vehicle table; left out until the session unloads: ex_2012 (12 cars)
[era]      StateUnloadSession Exiting: the lists the Time Trial guards left out are back in 1 entrant setup mode(s)
[era]      Time Trial fill at Sess_Setup_VehicleComponents made 27 vehicles where the budget expected 15
[era]      Time Trial session loads list f1_2025 (20 cars) of setup mode f1 (box pressed: f1_2025)
[era]      the Time Trial session's list ex_f1_2016 (setup mode swofty_f1) holds 24 cars; Time Trial holds at most 23 (the game adds 3 cars of its own to a table of 26); loading ex_f1_2017 instead
[era]      the Time Trial session's list ex_2012 (setup mode ex_2012_mode) holds 24 cars; Time Trial holds at most 23 (the game adds 3 cars of its own to a table of 26); its entrant setup mode offers no list of at most 23 cars; the session draws at most 23 cars (max_entrants 24 -> 23 until the session ends)
```

## Career seasons race in boxes

A career season names the box it races in with `career::Season::box`, and a fallback with `.or_else()`. The season uses that box's cars, looks and rules.

```cpp
// swofty.progressive: the 2017 season races in Classic Seasons' 2017 box, or the stock 2025 box without it
career::seasons.add("2017", {
    .data_group = "f1_career_progressive",
    .year = 2017,
    .box = content::packages["swofty.classic-seasons-2017/f1_2017"].or_else(content::packages["game/f1_2025"]),
});
```

- **The box for each season is chosen when the career is created.**
- **Two packs for one year:** when more than one installed box covers a year, the career setup page shows a choice for that year. It defaults to the box the career addon's season names. The choice is frozen at creation.
- **A career freezes each season's line-ups, ratings and rules when that season starts.** Cars, liveries, tracks and sounds come live from the season's box. Drivers and tracks installed mid-career join at the next season.
- **A career's owner can freeze everything at creation instead,** with `career::ModeRecord::freeze = career::Freeze::at_creation`. The career's hub can offer the same choice to the player as a **Freeze content** toggle.

See [Career snapshots](/std/scopes#career-snapshots) and [Career](/std/systems/career).

## The content registries

- `era.box`, `content.package`: this page.
- `vehicle`, `vehicle.package`, `vehicle.handling`: [Cars](/std/systems/cars).
- `engine_sound`: [Sounds](/std/systems/sounds).
- `livery`: [Liveries](/std/systems/liveries).
- `helmet`, `suit`, `gloves`, `wearable.season_set`: [Helmets, suits and gloves](/std/systems/wearables).
- `track`, `track.venue`, `track.data`, `track.per_package`: [Tracks](/std/systems/tracks).
- `team.identity`: [Teams](/std/systems/teams). `driver.identity`: [Drivers](/std/systems/drivers).
- `career.content`, `career.season`, `career.driver_ratings`: [Career](/std/systems/career).

Older addons' calls land in some of these. `swofty::era::add_box` registers an `era.box`, and a career content becomes a `career.content` record (see [Compatibility](/std/compatibility)).

## What the player sees

The launcher shows each GP/TT entry with the addons that built it, from the last session's `registry/effective.json` and its [origins](/std/reading):

```text
F1 2017                          Classic Seasons 2017
  Ferrari  engine: V10 Screamer  (mod.esp)
           livery: Monza 2017    (someone.liveries)
F1 2017 (V10)                    mod.esp, on Classic Seasons 2017
```

## Readiness and limits

- **Packages and boxes: ready.** The core's era picker (`era_picker.cpp`, up to 64 boxes) and the entrant pipeline are L2, mapped and verified in the game. Classic Seasons ships nine seasons on them.
- **The entrant list must be in `swofty_f1`,** the host setup mode the core narrows to the pressed season.
- **Shared lists compose.** Every season appends its entrant list to `content_sets[2025|2026].available_entrant_lists_0` and `entrant_setup_modes[swofty_f1]`. The core merges the appends.
- **Sound rows: boot.** When the game reads `audio.audio_car_data` after boot needs reverse engineering. A team's `engine_sound` applies at boot, to its car everywhere. To change one box only, give the box its own vehicle variant.
- **The box's track list: partial.** The list builder behind the track select screen (`ui.track_select.build`) needs reverse engineering. A guard covers tracks that don't fit.
- **Scale:** one season is about 700 records. Classic Seasons 2017, as an addon for 1.5, builds its season with about 700 operations: 99 add-row, 170 set and 430 put-record. They cover its car packages, `file.put`s for the `cs17_*.bdl` bundles, 430 raw audio records, and the rows for the entrant list, teams and pit times.
