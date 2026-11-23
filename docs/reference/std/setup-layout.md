---
title: Setup layout
description: Design for a per-scope layout of the car setup screen: which sliders show, in what order, under which headers, with their own labels and ranges. Not implemented in the core yet.
---

# Setup layout

::: warning Design
The pure part is in the core (`setup_layout.hpp`, host tested). The hooks that apply a layout to the screen are not written, and nothing here is tested in game.
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
- `min` and `max` go together. A row without them keeps the game's range.
- Two records whose scopes match one session: the later in load order wins, as for layers.
- A record whose layout fails `check_layout` is dropped with a `[setup-layout]` log line, and that scope sees the stock screen.

## Open points

- The array layout `BuildSetupGroupList` takes (u32 indexes, count at +0x28 of the caller's local) and the tab's name source are read from the decompile to L1; the hook needs one more pass to be exact.
- The value text of a row (`value_str`, and the `lng_cs_*_range` formats with `%.1f` and a unit key) is written by the game per slider; whether `unit` can be applied through `range_desc` alone, or needs the shared `lng_cs_*_range` keys set with the scope, is not read yet.
- The stock screen's eighth tab is the fuel tab on some cars; a layout may use it.
- A saved setup keeps all 42 steps, so a step saved in a scope returns the same row in that scope; outside it the same step is read by the stock slider of that index. Layouts should pick sliders whose stock use is inert in the scope's cars.
