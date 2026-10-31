---
title: Car physics and handling
description: How a car's handling is built from its handling file, every part you can change, when a change reaches the car, how to change a car live during a session, and how R&D, setup, damage and AI fit in.
---

# Car physics and handling

::: warning Experimental
Live car physics (`cars::live`, layers, `car.profile`) is experimental in 1.6.13. Nothing in it has run in game yet, and its shape can change before it is stable. A field whose game code has no independent verification yet stays off and logs `<field> off: needs <row>`.
:::

Every car on track is built from its **handling file**: the `.vtf` in the car's package (`teams/ferrari/handling/ferrari.vtf`). The file has one section per part of the car, such as Engine, F1Aero or F1WingDamage, and each section is a list of named values. Your addon changes those values in three ways:

- **Layer the handling record.** The change reaches every car built from that handling file, from the next session.
- **Change a car live.** Through `cars::live`, during a session, on one car or many, with the car's team and driver in hand.
- **Edit an upgrade block.** The R&D upgrades are blocks inside the same file, so changing one changes what R&D does.

A damage addon shows all three. It offers Light, Normal and Heavy damage in ADDON SETTINGS, and a change reaches the next race with no restart. A Live option goes further: it changes the cars on track at once.

```cpp
#include <swofty/registry.hpp>
#include <swofty/systems/cars.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    auto f1 = tagged<cars::Handling>(cars::handling.tag("c/f1_cars"));

    // Every F1 car built from now on: next session, every year.
    cars::handling.layer(f1, [](cars::Handling &h) {
        auto level = option<std::string>("wing_damage");          // "light", "normal", "heavy"
        if (level == "heavy") {
            h.wing_damage.speed_threshold = 2.0f;
            h.wing_damage.damage_scale = 0.000175f;
            h.wing_damage.full_detach_scale = 0.4f;
        }
    });
}
```

## The parts of a car

A built car has 23 parts, one per section of its handling file. `cars::Handling` has a member for each, named after the section, and each part has a field for every value in that section, named after the key. Here are the parts most addons change:

```cpp
namespace swofty::cars {

struct WingDamage {                        // F1WingDamage: the front wing
    float speed_threshold;                 // impacts below this speed do nothing (stock 3)
    float damage_scale;                    // damage per unit of impact (stock 0.0001)
    float chassis_damage_multiple;         // share of a body hit that reaches the wing (stock 0.3)
    float side_detach_scale;               // damage at which one side breaks off (stock 0.5)
    float full_detach_scale;               // damage at which the wing comes off (stock 1)
};

struct SuspensionDamage {                  // F1SuspensionDamage
    float speed_threshold;                 // stock 7
    float front_damage_scale;              // stock 0.001
    float rear_damage_scale;               // stock 0.0001
    float wheel_detach_scale;              // stock 1
    float parts_detach_scale;              // at least 1.5 x wheel_detach_scale: the game raises a lower value
    float linear_fracture_limit;           // stock 50
    float angular_fracture_limit;          // stock 30
};

struct Engine {                            // Engine
    Curve revs_power_spline;               // power in W against engine speed in rad/s (2025 Ferrari: 550 kW at 1191.5 rad/s)
    float idle_rotation_rate;              // rad/s
    float limiter_rotation_rate;           // rad/s
    float mass, inertia;
    float wear_ice_rate, wear_tc_rate, wear_mguh_rate, wear_ce_rate;   // power unit wear per second of use
    // the throttle maps, engine braking, temperatures and the rest of the section
};

struct FuelEcu {                           // FuelECU: one value per fuel mode (lean, standard, rich, max)
    std::array<float, 4> fuel_burn_scale, fuel_power_scale, fuel_revs_scale, temperature_scale;
};

struct Ers {                               // EnergyRecoverySystem
    float max_deploy_power;                // W (2025: 126.3 kW, 2026: 350 kW)
    float energy_store_capacity;           // J (4 MJ)
    float exhaust_power_coefficient, overtake_power_efficiency, max_brake_torque_mguk;
    std::array<ErsMode, 4> modes;          // none, medium, overtake, hotlap: deploy per gear
};

struct F1Aero {                            // F1Aero
    float front_coeff_lift_multiplier, rear_coeff_lift_multiplier;
    float body_drag_coeff_min, body_drag_coeff_max;
    Curve front_coeff_lift_vs_ride_height, rear_coeff_lift_vs_ride_height;
    // wing settings, DRS, slipstream, brake duct and radiator terms, ride-height clamps
};

struct Chassis {                           // Chassis
    float mass;                            // kg
    float moi_x, moi_y, moi_z;             // moments of inertia
    float com_y, com_z;                    // centre of mass
};

}
```

The other parts work the same way: `gearbox` (ratios R, N, 1st to 9th, final drive, shift times), `fuel_tank`, `brake_ecu` (brake torques, ABS and traction control curves), `steering`, `core_aero` (slipstreaming), `differential`, `suspension` (springs, dampers, anti-roll bars, camber and toe, and the tyre and brake records per axle), `turbo`, `tuning` (the setup ranges), `assists`, `plank`, `tyre_damage` and `body_damage`.

A key the handling file doesn't have keeps the game's default.

## When a change reaches the car

The game reads a handling file once when the file loads. It then builds each car from it when the car is put on track, which happens at every session load. A handling layer applies at that build, so:

- A layer reaches every car built after it, in every year, without a restart. Changing an option reaches the next session.
- A car that's already on track keeps its values until it's built again. Use `cars::live` to change it now.
- The game rebuilds a car's parts from the file during some events, such as handing a car from the player to the AI. Your layer is part of every rebuild, so it survives them.
- With Equal Performance on, every car is built from the base car's file, so a layer on the base car reaches all of them.

The core logs each build:

```text
[cars] built vehicle 0 (game/ferrari_2025, team game/ferrari, driver game/charles_leclerc): 23 parts, 3 layers
[cars] vehicle 0 handling: F1WingDamage /speed_threshold 3 -> 2 (william09leo.heavier-damage-realism)
```

## Changing a car live

`cars::live` gives you each car on track during a session: its index and the game's car. `car.control()` reads who drives it, human or AI, and its controller flags, fresh on every call (see [Who drives a car](./sessions#who-drives-a-car)), and `car.is_ai()` reads whether the AI drives it right now (the player's car too, while the AI has it).

Read and write a value with `get`, `set` and `scale`, by `Field` or by its Handling path. The core finds the car's part, decodes and re-encodes the game's scrambled values, and refuses a value outside the field's range with `Result::invalid`:

```cpp
#include <swofty/systems/cars.hpp>

using namespace swofty;
using cars::live::Field;

for (cars::live::Car &car : cars::live::cars()) {
    if (car.is_ai())
        continue;
    car.scale(Field::steering_lock, 1.2f);            // 20 % more steering lock, next frame
    car.set("fuel_tank.fuel_mass", 50.0f);            // kg, up to the tank's capacity
    car.set(Field::wing_full_detach_scale, 0.6f);     // the wing comes off sooner, next hit
}
```

A write lasts until the game rebuilds, resets or replays the car's parts. To keep a value, make a layer.

### Fields

```text
steering.max_steering_lock             rad, 0.01 .. 1.5708
gearbox.max_clutch_torque              multiple of engine torque, 0 .. 100
gearbox.final_drive_ratio              1 .. 20, rebuilds the ratio table
gearbox.gear_ratio_1 .. gear_ratio_9   0.1 .. 20, rebuilds the ratio table
differential.diff_on_throttle          0 .. 1, until the setup is applied again
differential.diff_off_throttle         0 .. 1
turbo.max_boost                        bar; no effect on a stock car
fuel_tank.fuel_mass                    kg, up to the tank's capacity
engine.vtf_limiter_rotation_rate       rad/s, 100 .. 2500, re-runs the derived limiter
f1_wing_damage.*                       speed_threshold, damage_scale, chassis_damage_multiple, side_detach_scale, full_detach_scale
f1_suspension_damage.*                 speed_threshold, front/rear_damage_scale, wheel/parts_detach_scale, linear/angular_fracture_limit

aero.lift_scale                        0 .. 10, multiplies the body lift, read every step
aero.drag_scale                        0 .. 10, multiplies the body drag
engine.power_scale                     0 .. 4, the engine drive torque, written to no game data
engine.limiter_offset_rpm              negative: the torque is cut that many rpm under the limiter
ai.grip_scale                          0 .. 4, the AI's table grip input, dry weather only
tyres.front.grip_scale                 the axle's GripMultiplier
tyres.rear.grip_scale
chassis.ballast_kg                     -300 .. 600, added to the VTF mass, then the chassis reset
brakes.bias                            0 .. 1, rewritten when a setup is applied
brakes.pressure                        0 .. 4
suspension.front.camber  .toe  .anti_roll  .ride_height_offset
suspension.rear.camber   .toe  .anti_roll  .ride_height_offset
```

`engine.power_scale`, `engine.limiter_offset_rpm` and `ai.grip_scale` are the core's own handlers: nothing is written to the car, and a car with no layer on them runs the game's code unchanged. The suspension fields are copied into the solver every frame, so a write reaches the car on the next frame.

A car's index is the game's vehicle id: dense from 0 in the order the cars were built, the safety car included. A `Car` from `cars()` also carries the game's car, and a call on it answers `Result::late` once that car is gone from its slot. `set` runs on the game's flow thread, so call it from a frame, setting or event handler; from another thread it answers `Result::late`. The values are read from the build the core's field table was mapped on: on any other build of the game `cars::live::available()` is false and `cars()` is empty.

## Layers

A layer is a value the core keeps and applies again after every `car.built`, `car.reset`, R&D upgrade replay and setup apply, so it outlives the game's own resets. It is removed when the `Layer` is destroyed:

```cpp
using namespace swofty;
using cars::live::Cars;
using cars::live::Layer;

static Layer power, lift, fuel;

SWOFTY_REGISTER
{
    power = Layer::scale(Cars::all(), "engine.power_scale", 0.9725f, /*priority*/ 0);
    lift = Layer::scale(Cars::ai(), "aero.lift_scale", 0.9f, 10);
    fuel = Layer::set(Cars::player(), "fuel_tank.fuel_mass", 60.0f);
}
```

Layers of one field compose by priority, lowest first, then in the order they were made: `set` replaces the value, `scale` multiplies it and `add` adds to it. The base is the game's value after its last reset (1 for `engine.power_scale` and `ai.grip_scale`, 0 for `engine.limiter_offset_rpm`). A composed value outside the field's range is clamped.

`Cars::all()` reaches every car, those built later too. `Cars::ai()`, `Cars::player()` and `Cars::car(car)` name the cars on track when the selector is made. `Layer::result()` is `Result::unavailable` when the field is off.

The C table (`SwoftyCarOps` version 2) has the same calls:

```c
const SwoftyCarOps *ops = SwoftyCoreCarOps();
uint32_t field = 0, token = 0;
ops->field("aero.drag_scale", &field);
ops->layer("my.addon", SWOFTY_CAR_ALL, field, SWOFTY_CAR_OP_SCALE, 0.96f, 0, &token);
ops->unlayer(token);
```

## Car profiles

`car.profile` records put layers on the cars a scope matches, from data, with no code. The core reads them at `car.built`, `car.reset`, the upgrade replay and whenever the session changes, drops its earlier profile layers for the car and adds the matching profiles' again. A career freezes the season's profiles at season start. A box names none: a profile's `within` selects the box, as a `rules.drs` record does.

```json
{
  "car.profile": {
    "race_engine": {
      "within": [{ "data_group": "f1_career_1990" }],
      "when": ["practice", "race"],
      "power_scale": 0.9725,
      "limiter_offset_rpm": -300,
      "ai_grip_scale": 0.87
    }
  }
}
```

That record is all of F1 1990's two native DLLs: the race engine (power x0.9725 and a cut 300 rpm under the limiter outside qualifying) and the AI's grip (x0.87 outside qualifying, in the dry).

```cpp
namespace swofty::cars {
struct Profile {
    json::Value within;                         // selector array: content, data_group, setup_mode, vehicle_package, ...
    std::vector<events::SessionType> when;      // empty: every session
    CarSelect cars;                             // indices; vehicle_package, vehicle and team wait for the car identity
    bool ai_only = false, player_only = false;
    int priority = 0;
    float power_scale = 1, limiter_offset_rpm = 0, lift_scale = 1, drag_scale = 1;
    float grip_scale = 1, ai_grip_scale = 1;
    std::optional<uint32_t> fuel_modes;         // off until the fuel mode call is verified
    json::Value extra;                          // every other key is a live field path
};
}
```

Every key the record does not name is a live field path. A number sets it; an object with `set`, `scale` or `add` composes:

```json
{
  "car.profile": {
    "heavy_aero_cars": {
      "when": ["race"],
      "ai_only": true,
      "aero.drag_scale": { "scale": 1.05 },
      "brakes.bias": 0.56,
      "chassis.ballast_kg": { "add": 12 }
    }
  }
}
```

Profiles compose by priority, then by record id. A profile whose `cars` names a vehicle package, vehicle or team is skipped and logged until the car's identity is mapped.

## Events

```cpp
events::on<cars::events::Built>().after([](cars::events::Built &e, const events::Context &) {});
events::on<cars::events::Reset>().after([](cars::events::Reset &e, const events::Context &) {});
events::on<cars::events::UpgradesReplayed>().after([](cars::events::UpgradesReplayed &e, const events::Context &) {
    float progress = e.progress;                // the development progress the block was replayed at
});
events::on<cars::events::SetupApplied>().after([](cars::events::SetupApplied &e, const events::Context &) {});
```

- `car.built` fires once for every car the game builds: at session load and when a restart builds the cars again.
- `car.reset` fires when the game re-reads a car's parts without building it again: during loading (twice for every car) and when it hands a car to the AI or back (T120).
- `car.upgrades.replayed` fires after the game replays an R&D upgrade block onto a car. The replay resets `aero.lift_scale`, `aero.drag_scale` and the fuel mode.
- `car.setup.applied` fires after a setup is written to the car: the differential, suspension, brakes and centre of mass.
- `car.parts.register` is declared and does not fire yet: it is off until `Car_FrontWing_Init` is verified.

The game builds cars on a loading thread, so the core holds each event and fires it on the next frame, on the flow thread, where `set` works. The core applies its layers and profiles to the car first, so a handler sees the layered values. A car already gone from its slot by then gets none.

`Reset::upgrade` is always false from the core; the upgrade replay is its own event.

## R&D upgrades

An R&D upgrade isn't a number in an array. Each upgrade type is a block inside the handling file, such as `radUpgradeEngine` or `radUpgradeFrontDownforce`, with the same sections and keys as the rest of the file. When the career fits an upgrade, the game reads the block on top of the car's values, scaled by the development progress `p` from 0 to 1:

- `*`: the value is multiplied by `(v - 1) * p + 1`.
- `+` and `-`: `v * p` is added or taken away.
- `/`: the value is divided by `(v - 1) * p + 1`.
- No operator, or `=`: the value is set to `v`, whatever the progress.

A block applies once per car build. The stock 2025 blocks change, among others:

```text
radUpgradeEngine            Engine/RevsPowerSpline +, FuelTank/FuelBsfcMin x0.9
radUpgradeFrontDownforce    F1Aero/FrontCoeffLiftMultiplier x1.26, FrontWingCoL1 x1.1
radUpgradeDrag              F1Aero/BodyDragCoeffMin x0.87, BodyDragCoeffMax x0.89
radUpgradeWeightReduction   Chassis/Mass -18
radUpgradeTyreWear          Suspension Tyre/UpgradeWearFactor x0.825
radUpgradeWearICE           Engine/WearICERate x0.5
```

A block is a list of changes, each a section, a key, an operator and a value:

```cpp
namespace swofty::cars {
struct UpgradeChange { std::string section, key; char op; float value; };   // op: '*', '+', '-', '/', '='
struct RadUpgrade { std::string name; std::vector<UpgradeChange> changes; };
}
```

Edit a block with the same layer, or add a new one for a department of your own:

```cpp
cars::handling.layer(f1, [](cars::Handling &h) {
    cars::RadUpgrade &engine = h.upgrade("radUpgradeEngine");        // the block, created if it's missing
    engine.set("FuelTank", "FuelBsfcMin", '*', 0.85f);               // a bigger fuel saving at full progress
    engine.set("Engine", "WearICERate", '*', 0.8f);
});
```

## Setup

The setup screen's sliders pick a point between a minimum and a maximum from the `tuning` part, then write it into the differential, suspension, brakes, engine braking, centre of mass and fuel load. The wing sliders are the exception: their range is the wing's own setting count in `f1_aero`. Widen or move a range with a layer:

```cpp
cars::handling.layer(f1, [](cars::Handling &h) {
    h.tuning.differential_on_throttle_multiple_min = 0.4f;   // a more open differential at the low end
    h.tuning.brake_bias_max = 0.7f;
});
```

## AI cars

AI cars use the same handling file, the same physics and the same R&D upgrades as the player. The difficulty setting changes how the AI drives, not its car: there's no hidden grip, power or drag boost. The AI-only values are in the handling file, so you can change them like any other:

- `brake_ecu`: the AI's stability control (`asc_strength_for_ai`, off in 2025) and `brake_migration_torque_ai`.
- `suspension` tyres: the AI's slip limits and grip factors on a wet or fast tyre.
- An extra tyre wear factor the player's car doesn't have.
- AI cars run their tyre contact every third physics step, where the player's runs every step.

For how the AI drives, see [AI](/std/systems/ai).

## Readiness

The parts, their fields and how they're stored are mapped from the game's code and checked independently. An in-game trace confirmed that cars are built once at session load and torn down when it ends, that the game rebuilds every car twice while a session loads and rebuilds one car when control of it changes, and the live values of engine speed, fuel, ERS, tyre wear, mass and the aero coefficients. Still to confirm in game: DRS, the fuel-mix and setup controls, career R&D upgrades, and how a car's index matches its participant. The `cars::live` fields are mapped from the game's code and checked on fake cars in the core's host test; none of them is tested in game yet. Their offsets are those of the game build the bindings document was made for: on any other build, a game patch included, the core's `same_build()` check turns `cars::live` off until the KB is re-derived for that build.

These stay off until the game code they rest on has an independent verification, and the core logs `<field> off: needs <row>` for each:

```text
ai.grip_scale                          site_Ai_TableGripInput
aero.lift_scale, aero.drag_scale       CarF1Aero.lift_scale, CarF1Aero.drag_scale
brakes.bias, brakes.pressure           CarBrakeEcu.brake_bias, CarBrakeEcu.brake_pressure
tyres.*.grip_scale                     CarSuspTyreType.grip_multiplier
suspension.*                           CarSuspAxle.setup_camber, setup_toe, setup_anti_roll, ride_height_offset
chassis.ballast_kg                     Car_Tuning_SetChassisZCom
refit_tyres                            Car_Tyre_SetWearFactor, CarChassis.tyre_physics_group
fuel modes                             Car_FuelEcu_SetMode_Thunk
car.parts.register                     Car_FrontWing_Init
```

`engine.power_scale`, `engine.limiter_offset_rpm`, `car.upgrades.replayed` and `car.setup.applied` rest on verified rows.

Credit: the front wing damage, suspension damage, fuel ECU and ERS layouts were cross-checked against findings shared by Will (william09leo), the author of Heavier Damage.
