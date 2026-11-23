---
title: Setup layout
description: "A per-scope layout of the car setup screen: which sliders show, in what order, under which headers, with their own labels and ranges."
---

# Setup layout

::: warning Experimental
The `setup.layout` registry is experimental and its fields may change. The hooks that apply a layout are tested in game in Time Trial.
:::

A `setup.layout` record gives the car setup screen of one scope its own sliders, order, section headers, labels and displayed ranges. Outside the scope the screen is the game's.

## What the screen is

`garage_monitor_car_settings.ui` (flow_car_monitor.erp) has no row per slider. Its tab strip `setup_group_types` (`viewWidth` 8, data `setup_groups`) and its list `setup_data` (nine generic `entry` rows, data `selected_setup_group.group`, count `selected_setup_group.count`) show whatever the VM publishes:

- `Game_CarSetupVm_PublishSetupCounts` (0x140c14220) calls `Game_CarSetupVm_BuildSetupGroupList` (0x140c14a30) once per tab with an array of slider indexes. A slider in no array has no row. Some arrays depend on the car: engine braking, brake pressure and ballast appear only when the car's `F1Tuning` min differs from its max.
- `Game_CarSetupVm_PopulateGarageMonitorSettings` (0x140c17b80) makes one entry node per `car_setup.sliders` index (42). Each node holds plain VM data: `name`, `desc`, `value_str`, `min_value_str`, `max_value_str`, `range_desc`, `value_format`, `enabled`, and the numbers the slider binds, `min`, `max`, `step` and the value.
- The numbers come from three global tables of cells, read by the VM on every populate and by the setters: `car_setup.sliders` max (0x14605a428), min (0x14605a578) and steps (0x14605a6c8), one handle per slider, the value at `*handle + 8`. The value node is `lerp(min, max, step / steps)`, and `Car_Setup_SetSliderValue` (0x140b782f0) turns the screen's number back into a step with the same cells. `Car_Setup_ApplyTuning` divides the step by the steps cell for the physics and never reads min or max (the physics range is the car's `F1Tuning`).

Nothing in the UI archive names a slider, so the layout needs no archive patch.

## How a layout is applied

| Wish | Mechanism |
|---|---|
| Show, hide, order, group | The core replaces the index arrays `PublishSetupCounts` passes to `BuildSetupGroupList` with the layout's sections. A slider not listed has no row; the list order is the array order. |
| Section header | The tab title is a `lng_cs_<tab>_title` key of the game; the core sets it with a string override while the scope is active and clears it after (`swofty::lang`). One section per tab, 8 at most. |
| Row label and description | Written into the entry node's `name` and `desc` after the VM populates, and again after `Car_Setup_OnValueChanged` (0x140c1e690) rewrites the entry. |
| Range of a row | The core swaps the row's min and max cells while the scope is active and puts them back when it ends. The VM, the slider and the setter then agree on the number, and the physics is untouched. The steps cell stays: the physics depends on it. |
| Unit | Written to the entry's `range_desc` and `value_format`. |
| Show a hidden slider | List it. Ballast (25) is a physical slider (chassis centre of mass, `F1Tuning` COM-Z range) and shows once listed; sliders 37 to 40 are inert unless their `F1Tuning` ranges differ. |

The scope switch (`ScopeSwitch`) runs when the setup screen's VM initialises (`Game_CarSetupVm_Initialise`, 0x140c1b1a0), where the core already guards the car pointer: it picks the layout whose `within` matches the session, swaps the cells and strings, and puts them back when the VM is destroyed or the session ends. Entering twice never stacks swaps. A session no layout matches sees the stock screen.

## The record

```json
{
  "setup.layout": {
    "f1_1990": {
      "within": [ { "content": "swofty.f1-1990/f1_1990_gp" }, { "content": "swofty.f1-1990/f1_1990_tt" }, { "data_group": "f1_career_1990" } ],
      "sections": [
        { "tab": 0, "title": "Aerodynamics", "rows": [
          { "slider": "front_wing", "name": "Front wing" },
          { "slider": "rear_wing",  "name": "Rear wing" } ] },
        { "tab": 1, "title": "Chassis", "rows": [
          { "slider": "ballast", "name": "Ballast", "desc": "Weight forward or back", "min": -10, "max": 10, "unit": "kg" } ] },
        { "tab": 4, "title": "Brakes", "rows": [
          { "slider": "brake_bias", "name": "Brake bias" } ] },
        { "tab": 7, "title": "Fuel mix", "rows": [
          { "slider": "brake_cooling_front", "name": "Lean",     "min": 1.00, "max": 1.20, "unit": "λ" },
          { "slider": "brake_cooling_rear",  "name": "Standard", "min": 0.90, "max": 1.10, "unit": "λ" },
          { "slider": "brake_duct_front",    "name": "Rich",     "min": 0.84, "max": 1.00, "unit": "λ" },
          { "slider": "brake_duct_rear",     "name": "Max",      "min": 0.80, "max": 0.96, "unit": "λ" } ] }
      ]
    }
  }
}
```

- `within` takes the selectors of content and layers (`content`, `owner`, `data_group`, `mode`, `setup_mode`, `any`, `not`). A box is not a setup mode (every box shares `swofty_f1`): scope on the box's entrant lists or its content id, and the career's `data_group`.
- `slider` is a `car_setup.sliders` label or an index 0 to 41; a label such as `front_wing` is one slider. Each slider appears once; one section per tab; one to ten rows per section; a section needs a title and a row a name (`check_layout`).
- `min` and `max` go together. A row without them keeps the game's range. `min` may be above `max`: the row then reads from the high number down (a ballast row shown as 42 to 40 percent).
- `steps` (1 to 100) gives a row its own number of positions, on any slider. The cell is swapped with min and max while the scope is active and restored when it ends. ApplyTuning divides the stored step by the same steps cell, so the car's physics follows the fraction through the slider's F1Tuning range; the brake sliders 37 to 40 need the scope's handling to pin their F1Tuning ranges to min == max (the stock constructor does so for the cooling pair, 1050 and 900, and leaves the duct pair at 0 to 1). A saved setup and a preset keep raw step indexes, not numbers: a layout that changes a slider's steps owns the presets and saved setups of that slider, and a step saved above the new count is clamped to the top.
- Two records whose scopes match one session: the later in load order wins, as for layers.
- A record whose layout fails `check_layout` is dropped with a `[setup-layout]` log line, and that scope sees the stock screen.

## Row text

Optional fields of a row control the text beside the bar and the right-hand text. With any of them, or a `min` and `max`, the row's text is its own and the game's stock number and its `Soft/Firm` or `Min/Max` range text never show.

```json
{ "slider": "front_susp", "name": "Front spring", "min": 1050, "max": 1350, "steps": 6, "format": "{v} lb/in", "group": true }
{ "slider": "rear_susp", "name": "Rear spring", "min": 1.5, "max": 3.0, "steps": 6, "format": "{v} kg/mm", "decimals": 2, "range_text": "Soft to firm" }
{ "slider": "diff_preload", "name": "Final drive", "labels": ["Short", "Medium", "Long", "Very long"] }
```

- `format`: the value text, `{v}` the number at the step, `lerp(min, max, step / steps)`.
- `decimals` (0 to 3): digits of `{v}`; absent, the fewest that show the step size exactly. `group`: thousands separator (`1,050`).
- `labels`: one text per step, shown instead of the number; steps become `labels.length - 1` (a `steps` that disagrees is refused).
- `range_text`: a string replaces the right-hand text, an empty string hides it; absent, it is the row's min to max through `format` (`1,050 - 1,350 lb/in`), or the first and last label.

The text is written per row on the entry node (`value_str`, `range_desc` with `range_localise` false), so rows that share a range key in the game each get their own text. The entry nodes are rebuilt by the game every time the screen opens, so leaving the scope needs no restore beyond the cells.

## Open points

- Text path: the game builds each row's value text with a view-model formatter (`value_format` node, `unit_type` 'none', `decimal_places`) that writes `value_str`, and the right-hand text from one shared `lng_cs_*_range` key per group (`Soft {min} - {max} Firm`, `Min {min} - {max} Max`) into `range_desc`, localised by `range_localise`. A layout row's own text is written to those entry nodes after populate and after each value change (see Row text). Whether the formatter rewrites `value_str` after the change handler returns is checked in game with `SWOFTY_SETUP_LAYOUT_LOG=1`.
- The stock screen's eighth tab is the fuel tab on some cars; a layout may use it.
- A saved setup keeps all 42 steps, so a step saved in a scope returns the same row in that scope; outside it the same step is read by the stock slider of that index. Layouts should pick sliders whose stock use is inert in the scope's cars.

## Hooks and the builds they run on

The layout hooks (`native/src/core/hooks/setup_layout_hooks.cpp`) find three sites by byte scan, each on its own, on Steam 1.0.145 (build 26566308), where the KB addresses are not valid: `Game_CarSetupVm_BuildSetupGroupList` (0x140c154f0 there), `Game_CarSetupVm_PopulateGarageMonitorSettings` (0x140c18640) and the three `car_setup.sliders` cell arrays (leas inside the populate function, 0x150 apart). A site that does not resolve, or a hook that fails, logs one `[setups] layout:` line and leaves the stock screen. The layouts are the records of the `setup.layout` registry (`registry/setup_layout.json` in an addon, schema `packages/sdk/std/registries/setup.layout/1.0.json`, experimental). They are read each time the setup screen opens, and the one whose `within` matches the current facts (the last in load order; no `within` matches everywhere) is the screen. `SWOFTY_SETUP_LAYOUT_FILE` names a JSON file with the same `setup.layout` record, read in addition to the registry for development, and `SWOFTY_SETUP_LAYOUT_LOG=1` logs what the screen did.

`tab` is the game's own tab slot, a number or its name: 0 `fuel_load`, 1 `aerodynamics`, 2 `transmission`, 3 `suspension_geometry`, 4 `suspension`, 5 `brakes`, 6 `tyres`, 7 `weight_distribution`. `slider` is the SDK name or the stock database label.

Row names go on the game's own text key for the slider (`lng_cs_*`), so the diff rows 2 and 3 have none yet. The unit goes on the group's shared range key (`lng_cs_brake_ch_range` for 37 and 38, `lng_cs_brake_cd_range` for 39 and 40, one each for fuel load, ballast and the two brake sliders), and only where every row sharing the key gives the same unit. `desc` is one text per group in the game and is not applied.
