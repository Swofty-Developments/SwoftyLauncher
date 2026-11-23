---
title: Setup rows
description: Design for an addon's own rows in the car setup screen, with its own labels and tab, inside a scope. Not implemented in the core yet.
---

# Setup rows

::: warning Design
The pure part is in the core (`setup_rows.hpp`, host tested). The hooks that put the rows on the screen are not written, and nothing here is tested in game.
:::

An addon declares that car setup sliders show in the setup screen with its own label and description, in a tab it picks, only while a scope matches. The F1 1990 mix rows are the first user: sliders 37 to 40 (the four brake cooling and duct sliders, which no stock car shows) become Lean, Standard, Rich and Max.

## What the screen is

The setup screen (`garage_monitor_car_settings.ui`, flow_car_monitor.erp) has no row per slider. Its `setup_data` list shows nine generic entries from the selected group (`setup_data` in the UI is bound to `selected_setup_group.group`, `datasize` to `selected_setup_group.count`), and its `setup_group_types` list is the tab strip, `viewWidth` 8: at most eight tabs, one per group.

The VM builds it (`Game_CarSetupVm_PopulateGarageMonitorSettings` 0x140c17b80 and `Game_CarSetupVm_PublishSetupCounts` 0x140c14220):

- One entry node per `car_setup.sliders` index, 42 of them, named by the slider (`name`, `desc`, `value_str`, `min_value_str`, `max_value_str`, `enabled`, `can_move_left`, `can_move_right`, `value_format`, `range_desc`). The game fills them for every slider.
- Groups are published by `Game_CarSetupVm_BuildSetupGroupList` 0x140c14a30, one call per group with an array of slider indexes. A group holds a slider only when the game's code puts it there, and some entries depend on the car: the engine braking, brake pressure and ballast sliders appear only when the car's `F1Tuning` range for them has a minimum different from its maximum (`PublishSetupCounts` tests CarTuning +0x58/+0x5c, +0xe8/+0xec and +0x28/+0x2c).
- Sliders 19 to 24, 28 to 41 (brake migration, ramp diff outside handling version 100, inter and wet pressures, brake cooling and ducts) are in no group, and the game's English text has no label for them (no `lng_cs_*` key for cooling, duct, migration, mid corner, preload or high speed).

So no UI archive is patched: the list is generic. The feature changes the VM's data.

## The record

```json
{
  "setup.row": {
    "mix_lean":     { "slider": "brake_cooling_front", "group": "brakes", "name": "Lean",     "desc": "Fuel mix lambda",
                      "within": [ { "owner": "swofty.f1-1990" }, { "data_group": "f1_career_1990" } ] },
    "mix_standard": { "slider": "brake_cooling_rear",  "group": "brakes", "name": "Standard", "within": [ ... ] },
    "mix_rich":     { "slider": "brake_duct_front",    "group": "brakes", "name": "Rich",     "within": [ ... ] },
    "mix_max":      { "slider": "brake_duct_rear",     "group": "brakes", "name": "Max",      "within": [ ... ] }
  }
}
```

- `slider` is a slider name or an index 0 to 41.
- `group` is one of the eight tab names the game has (`aerodynamics`, `transmission`, `suspension_geo`, `suspension`, `brakes`, `tyres`, `weight_distribution`, `fuel`), or a tab title the addon sets with a string override while the scope matches.
- `within` takes the selectors of the other registries. A box is not a setup mode: every box shares `swofty_f1`, so a record scopes on the addon's box gate or `data_group`, as Rich Fuel Mix does.
- Two records in scope cannot share a slider (`check_setup_row`).

## What the core does

1. After `PublishSetupCounts` builds the groups, the core appends the slider indexes of the rows in scope to the group each row names (`merge_group`). A tab holds ten indexes (`SETUP_GROUP_ENTRIES_MAX`); the entry list scrolls.
2. After `PopulateGarageMonitorSettings`, the core writes `name` and `desc` into the entry node of each row's slider (`row_of_slider`), and `enabled`, so the slider is not locked.
3. The row's number is `car_setup.sliders` min, max and steps for that index, global to every car; the screen shows `min + (max - min) * step / steps` (`display_value`). The core does not change the table: it writes `min_value_str`, `max_value_str` and `value_str` for the entry from the row's own range (`min`, `max`, `format` in the record) when it differs.
4. The step is stored with the setup, in saved setups, career data and netplay (the persisted block is all 42 ints), and arrives on `car.setup.applied` ([setup steps](/std/systems/car-physics#setup-steps)).

Outside the scope the VM is untouched: no extra index, no strings written.

## Open points

- The group argument layout (an index array and a count at +0x28, `PublishSetupCounts` locals) and the entry node's `name` setter still have to be read to the hook's level; the groups array is static data in `.rdata`.
- Whether the eight tabs are all used by the stock screen (the names table `0x14568ec68` has eight): when they are, rows go into an existing tab.
- The entry's value text updates on `Car_Setup_OnValueChanged` (0x140c1e690) from the same table: the core re-writes the strings there too.
