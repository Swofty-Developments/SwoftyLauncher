---
title: Power units, ERS and DRS
description: Power units, engine suppliers, DRS rules and engine component rules in code, with the engine wear, grid penalty and DRS events.
---

# Power units, ERS and DRS

With the power system you give cars an engine, set when DRS opens, and change the engine component rules. F1 1990's cars, for example, run a normally aspirated V10 or V12 with no ERS and no DRS.

A **power unit** is the engine, turbo, fuel and ERS blocks a car's handling file carries. A **supplier** is the engine maker a car shows. **Component rules** are how many engine parts a driver gets per season, how fast they wear, and the grid drops for using more.

::: info In 1.6.4
`swofty/systems/power.hpp` has `power::DrsRule` and `power::drs_rules` (`rules.drs`), and a Grand Prix or Time Trial box's DRS rule applies: see [DRS per box](#drs-per-box). Power units, suppliers, component rules and the events below are the design; they come in a later header.
:::

## The header

<Split>

`swofty/systems/power.hpp` has four registries:

- `power::units`: a power unit, applied into every car that points at it (`power_unit`);
- `power::suppliers`: an engine supplier row (`engine.supplier`);
- `power::drs_rules`: a DRS-allowed lap group (`rules.drs`);
- `power::component_rules`: a component set: allocation, grid drops, wear (`rules.pu_components`).

A car points at its unit through `cars::Vehicle::power_unit`.

<template #example>
<Example root="swofty-sdk" code-file="include/swofty/systems/power.hpp">
<template #code>

```cpp
#pragma once
#include <swofty/registry.hpp>
#include <swofty/events.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/drivers.hpp>
#include <swofty/systems/sessions.hpp>

namespace swofty::power {

struct Unit {
    VtfSection engine;                         // Engine: RevsPowerSpline, rev limiter
    VtfSection turbo;                          // Turbo: MaxBoost, TurboMode
    VtfSection fuel_tank;                      // FuelTank
    VtfSection fuel_ecu;                       // FuelECU: burn, power, revs and temperature scales per Lean/Standard/Rich/Max
    std::optional<VtfSection> ers;             // EnergyRecoverySystem; nullopt: the car has no ERS (as F2 cars don't)
    VtfSection upgrades;                       // radUpgradeEngine, ERS, FuelConsumption, WearES, WearMGUK, WearMGUH
};
inline Registry<Unit> units{"power_unit", { .applies = Applies::boot }};

struct Supplier {
    Text name;                                 // the supplier name
    std::string nationality;                   // "japan", "italy", ...
    std::string rev_dial;                      // the HUD rev dial and rev lights fields
    StatId stat_id = StatId::automatic;        // an entrants.engines row
};
inline Registry<Supplier> suppliers{"engine.supplier", { .applies = Applies::boot }};

struct DrsRule {                               // in 1.6.4: a drs.drs_allowed_lap group (see DRS per box)
    int race_start = 1;                        // RaceStart lap_number
    int red_flag_restart = 1;                  // RedFlagRestart lap_number
    int safety_car_end = 1;                    // FullSafetyCarEnd lap_number
};
inline Registry<DrsRule> drs_rules{"rules.drs", { .applies = Applies::boot }};

enum class Component { ce, es, gearbox, ice, mguh, mguk, tc };

struct ComponentRule {
    int allocation;                            // parts per season
    std::map<std::string, float> wear_rates;   // wear_rates.*, per session kind
};

struct ComponentRules {
    int season;                                // the game picks its set by season only
    std::map<Component, ComponentRule> components;   // "25": all seven; "26": no mguh
    int first_new_grid_drop = 10;              // places for the first part over the allocation
    int subsequent_grid_drop = 5;              // places for each part after that
    int parc_ferme_grid_drop = 22;             // places for a change under parc fermé
};
inline Registry<ComponentRules> component_rules{"rules.pu_components", { .applies = Applies::boot }};

namespace events {

struct Wear {                                  // Game_EngineMgmt_ApplySessionWear
    static constexpr const char *point = "career.engine.wear";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    Ref<drivers::Identity> driver;             // whose parts wear
    sessions::Session session;                 // the session that ends
    Component component;                       // the part
    float wear;                                // the wear the game adds; before handlers may change it
};

struct GridPenalty {                           // Game_EngineMgmt_AddGridPenalty
    static constexpr const char *point = "career.engine.grid_penalty";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    Ref<drivers::Identity> driver;             // who drops
    int places;                                // before handlers may change it; replace = no penalty
    Component reason;                          // the part over the allocation
};

struct DrsAllowed {                            // the per-car decision in Game_DRS_UpdateCars; unavailable (L1)
    static constexpr const char *point = "session.drs.allowed";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t car;                              // vehicle index in the session
    bool allowed;                              // before handlers may change it
};

} // namespace events

Result add_grid_penalty(Ref<drivers::Identity> driver, int places, Component reason);
Result set_component_wear(Ref<drivers::Identity> driver, Component c, float wear);

} // namespace swofty::power
```

</template>
</Example>
</template>
</Split>

## A 1990 power unit without ERS

<Split>

F1 1990's cars run a normally aspirated engine with no ERS. The unit is a variant of the stock 2025 unit: its own engine spline and fuel map, and no `EnergyRecoverySystem` section, which is how the game has no ERS (F2 cars have none). There's no ERS-off switch.

The DRS rule opens DRS on lap 200, which means never. F1 1990 has its own setup mode and sets the rule on that mode. Grid penalties for engine parts don't exist in 1990, so a scoped handler replaces them.

<template #example>
<Example root="swofty.f1-1990" code-file="native/src/power.cpp" data-file="registry/power.json" files="swofty.json, native/CMakeLists.txt, pu/v12_engine.vtf, pu/v10_engine.vtf, pu/fuel_1990.vtf">
<template #code>

```cpp
#include <swofty/registry.hpp>
#include <swofty/events.hpp>
#include <swofty/systems/power.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/sessions.hpp>

using namespace swofty;

static const events::Selector k1990 = events::vehicle_package("swofty.f1-1990/f1_1990");

SWOFTY_REGISTER
{
    auto stock = power::units["game/pu_2025"];

    auto v12 = power::units.variant("v12_1990", stock, [](power::Unit &u) {
        u.engine = VtfSection::read(file("pu/v12_engine.vtf"));
        u.fuel_ecu = VtfSection::read(file("pu/fuel_1990.vtf"));
        u.ers.reset();                                     // no EnergyRecoverySystem section: no ERS
    });
    auto v10 = power::units.variant("v10_1990", v12, [](power::Unit &u) {
        u.engine = VtfSection::read(file("pu/v10_engine.vtf"));
    });

    power::suppliers.add("honda", { .name = "lng_f1990_engine_honda", .nationality = "japan" });
    power::suppliers.add("ferrari", { .name = "lng_f1990_engine_ferrari", .nationality = "italy" });

    auto never = power::drs_rules.add("never", { .race_start = 200, .red_flag_restart = 200, .safety_car_end = 200 });

    sessions::modes.layer(sessions::modes["swofty.f1-1990/f1_1990"], [=](sessions::Mode &m) { m.drs = never; });

    cars::vehicles.layer(under<cars::Vehicle>("swofty.f1-1990/"), [=](cars::Vehicle &v) {
        v.power_unit = v.label.starts_with("f1990_ferrari") ? v12 : v10;
    });
}

SWOFTY_ON_LOAD
{
    // 1990 has no engine allocation: no grid drops for parts
    events::on<power::events::GridPenalty>().in(k1990)
        .replace([](power::events::GridPenalty &p, const events::Context &) {
            swofty::log("[f1-1990] no grid penalty for %s (%d places)", p.driver.id().data(), p.places);
            return true;
        });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
{
  "power_unit": {
    "v12_1990": { "extends": "game/pu_2025", "engine": "pu/v12_engine.vtf", "fuel_ecu": "pu/fuel_1990.vtf", "ers": null },
    "v10_1990": { "extends": "swofty.f1-1990/v12_1990", "engine": "pu/v10_engine.vtf" }
  },
  "engine.supplier": {
    "honda":   { "name": "lng_f1990_engine_honda", "nationality": "japan" },
    "ferrari": { "name": "lng_f1990_engine_ferrari", "nationality": "italy" }
  },
  "rules.drs": {
    "never": { "race_start": 200, "red_flag_restart": 200, "safety_car_end": 200 }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] register: swofty.f1-1990 (f1_1990_pu.dll) 2 power_unit, 2 engine.supplier, 1 rules.drs,
           1 layer on session.mode, 1 selector layer on vehicle (early stage, 0.6 ms)
[registry] vehicle: selector under swofty.f1-1990/ from swofty.f1-1990 matched 26 records
[registry] applied at boot: power_unit 2 (26 handling files in an overlay archive), engine.supplier 2, rules.drs 1
[events] career.engine.grid_penalty in vehicle package swofty.f1-1990/f1_1990: replace by swofty.f1-1990 -> handled
[f1-1990] no grid penalty for swofty.f1-1990/alain_prost (10 places)
```

The core writes each unit into the handling files of the cars that use it, and adds them to the game's asset manifest at boot in an overlay archive. The data version has no grid-penalty handler, and no selector over the cars. Each 1990 car then names its unit in its own `vehicle` record.

## DRS per box

Every Grand Prix and Time Trial box shares the `swofty_f1` setup mode. A box's DRS rule is a `rules.drs` record, and the box's `content.package` names it in `rules.drs`. A 2010 season with no DRS:

```json
// community.f1-2010: registry/content.json
{
  "rules.drs": {
    "never": { "race_start": 200, "red_flag_restart": 200, "safety_car_end": 200 }
  },
  "content.package": {
    "f1_2010": {
      "title": { "literal": "F1 2010" },
      "entrant_list": "f1_2010",
      "season": 2010,
      "rules": { "drs": "community.f1-2010/never" }
    }
  }
}
```

The same in code, as a layer on another addon's box:

```cpp
// community.f1-2010-rules: native/src/drs.cpp
#include <swofty/registry.hpp>
#include <swofty/systems/content.hpp>
#include <swofty/systems/power.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    // DRS never opens in a race in this box; every other box keeps its own rule
    auto never = power::drs_rules.add("never", { .race_start = 200, .red_flag_restart = 200, .safety_car_end = 200 });

    content::packages.layer(content::packages["community.f1-2010/f1_2010"], [=](content::Package &p) {
        p.rules.drs = never;
    });
}

SWOFTY_ADDON()
```

```text
[apply] rules.drs boot: 1 applied, 0 put back, 0 refused
[apply] rules.drs boot:   9 database record(s) staged
[apply] rules.drs boot:   rules.drs community.f1-2010/never: group swofty_drs_never_2f7b7102 from f1_2024, race start lap 200, red flag restart 200, safety car end 200
[registry] applied at boot: ..., rules.drs 1 (9 database records), ...
[drs] box f1_2010 (community.f1-2010/f1_2010): swofty_f1 DRS group swofty_drs_never_2f7b7102 (rules.drs community.f1-2010/never: race start lap 200, red flag restart 200, safety car end 200)
[drs] box f1_2017 (swofty.classic-seasons-2017/f1_2017): no DRS rule; swofty_f1 keeps its own DRS group (f1_2024)
```

### The three laps

The three numbers are the game's `lap_number` fields of a `drs.drs_allowed_lap` group:

```cpp
power::DrsRule stock;                       // the stock f1_2024 group every F1 mode uses: 1, 1, 1
stock.race_start = 1;                       // RaceStart: DRS is allowed once a car's lap count reaches this
stock.red_flag_restart = 1;                 // RedFlagRestart: the lap of the red-flag restart + this
stock.safety_car_end = 1;                   // FullSafetyCarEnd: the lap the full safety car ends on + this
power::DrsRule never{ .race_start = 200, .red_flag_restart = 200, .safety_car_end = 200 };   // never in a race
```

- A missing field is the stock 1. A lap is 0 to 9999; the launcher and the core refuse anything else.
- The game's other groups: `f2_2023` (1, 1, 1) for F2, `f1_elimination` (0, 1, 1) for Elimination.
- The rule gates race sessions only. Practice, qualifying and Time Trial allow DRS in the zones as before.
- A car still needs a DRS zone (the track's `drs_det` and `drs_act` markers), the dry, and to be within one second at the detection point. 2026-regulation cars (`is_26_aero_regulations` on the vehicle package) run the 2026 active aero instead and ignore the rule.

### How it applies

- **Boot.** Each `rules.drs` record becomes one `drs.drs_allowed_lap.drs_allowed_lap_group` row, a copy of `f1_2024` labelled `swofty_drs_<key>_<8 hex digits of the record id's hash>`, with its three laps set. On a boot where the game database is not reachable, the records are refused with `the game database is not reachable at this boot; no rows applied` and every box runs `f1_2024`.
- **Box press.** When an addon box is pressed, the era picker narrows `swofty_f1` to the box's entrant list and points `swofty_f1`'s DRS group at the box's rule. A stock box, a box without a rule, or a rebuilt picker with no addon box selected puts the game's group back.
- **Setup mode rebuild.** `Sess_Apply_ContentAvailability` rebuilds the game setup modes and frees the old ones. At its entry the core drops the swaps it holds without writing them back, along with the era picker's narrowing and the Time Trial hold, and logs `[era] the game rebuilds its setup modes (Sess_Apply_ContentAvailability): dropped ...` once. The next box press or picker build applies the box's rule to the new modes.
- **Session.** The game reads the mode's group at session load, when a full safety car ends and at a red-flag restart.
- **Unchanged:** careers (they run their own modes), a box with its own setup mode (its mode row names its group; set it with a [database operation](/reference/ops/database)), and every addon without `rules.drs` records.

## Swapping a unit into other cars

A unit applies into every car that points at it. Classic Seasons 0.5.3 puts the stock 2025 Ferrari's blocks into every car of every season:

```cpp
cars::vehicles.layer(under<cars::Vehicle>("swofty.classic-seasons-"), [](cars::Vehicle &v) {
    v.power_unit = power::units["game/pu_2025"];
});
```

```text
[registry] vehicle: selector under swofty.classic-seasons- from swofty.classic-seasons matched 80 records
```

Checked over UDP after the swap: ICE 550 kW plus MGU-K 120 kW, a 4 MJ store, and DRS for every car.

## Component rules

```cpp
// the 2026 set, with one more power unit per season and softer drops
power::component_rules.layer(power::component_rules["game/components_26"], [](power::ComponentRules &r) {
    r.components[power::Component::ice].allocation += 1;
    r.first_new_grid_drop = 5;
});
```

## Events and operations

```cpp
// half the engine wear in practice
events::on<power::events::Wear>().when(events::session_type({ events::SessionType::practice }))
    .before([](power::events::Wear &w, const events::Context &) { w.wear *= 0.5f; });

// a stewards' addon hands out its own drop
power::add_grid_penalty(drivers::identities["game/max_verstappen"], 5, power::Component::gearbox);
power::set_component_wear(drivers::identities["game/max_verstappen"], power::Component::gearbox, 0.0f);
```

## Readiness and limits

- **Applies at boot: ready.** Unit swaps shipped (Classic Seasons, F1 1990) and were verified. The handling-file round trip is proven.
- **DRS per box: implemented in 1.6.4, waiting on the in-game check.** The game setup mode's DRS group field (`GameSetupMode` +0x1e0) and its parser are L2; the DRS readers (`Game_DRS_OnSessionLoad`, `Game_DRS_ResetAllowedLap`, the safety car and red flag handlers) and the current-mode pointer are L1.
- **Events: partial.** Engine management (`ApplySessionWear`, `AddGridPenalty`) is L2. DRS and ERS decisions are L1 or unknown: `session.drs.allowed` (the per-car decision in `Game_DRS_UpdateCars`) is unavailable. Also unknown: the component-set selector, the ERS deploy and harvest writers, and overtake-mode gating.
- **Fixed in the game:** the one-second DRS detection gap (set at session load) and each track's DRS zones (its track markup). `drs_capable` on each vehicle row is read at boot; what reads it in a session is not traced.
- **The game picks a component set by season only.** Choosing one per career needs the selector found.
- **AI tables carry baked ERS data per car** (`ers_boost_data`, `ers_use_granular`). A new unit can outrun the baked tables.
- **The panel slot site: experimental, no std operation yet.** `site_Sess_PuPanel_SlotHolder` in `swofty/game.hpp` sits in the slot loop of the power-unit panel's view model build (the garage monitor and laptop lists): `rbp` is the slot (0 ICE, 1 MGU-K, 2 MGU-H, 3 ES, 4 TC, 5 CE, 6 event gearbox) and `rax` its holder. The panel lists a slot only when the holder's fitted index (+0x30) is below its part count, so a site mixin can leave a slot out; F1 1990 leaves out the MGU-H row. The build (`Sess_PuPanel_BuildComponents`) and the row filler (`Sess_PuPanel_FillComponent`) are in the KB but not exported.
- **The HUD dial differs per package** (`hud_dial` modern, 26, f2).
- **Live physics values are XOR-obfuscated.** Units apply to the handling file before it loads.
- **2025 fuel mixes:** only Lean differs. In 2026 and F2 cars the mixes have no effect.
