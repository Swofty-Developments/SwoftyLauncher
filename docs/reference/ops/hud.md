---
title: HUD elements
description: Add sprites, texts, level bars, gauges and sweeps to the game's HUD dials with hud.element records and operations.hud, and what the composer does with them.
---

# HUD elements

A `hud.element` record puts art and live values into one of the game's three HUD dials. The build composes the records of every enabled addon into one grown dial scene per dial, one template pass and one editor preview, and writes ready overlay archives.

```json [swofty.json]
"operations": {
  "hud": ["registry/hud.json"]
}
```

## A record in full

A Halo-like dial: a base, a speed text, a boost lamp, a level bar, a battery gauge and an RPM sweep, with the editor preview and an F2 variant.

```jsonc [registry/hud.json]
{
  "hud.element": {
    "halo": {
      "dial": "f1_26",                              // f1_26, f1_25 or f2: the car class dial it grows
      "stock": "park",                              // keep | hide | park (default: park in f1_26, hide elsewhere)
      "sprites": {                                  // PNG art; sizes are multiples of 4
        "base":  { "png": "art/base.png",  "bounds": [-100, -48, 110, 38] },   // x0 y0 x1 y1 in HUD units
        "lit":   { "png": "art/lit.png",   "bounds": [-90, -30, -74, -22] },
        "fill":  { "png": "art/fill.png",  "bounds": [-20, 8, -4, 16] },
        "mask":  { "png": "art/mask.png",  "bounds": [10, -30, 42, 2] },
        "solid": { "png": "art/solid.png", "bounds": [-4, -4, 4, 4] }
      },
      "elements": [
        { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 },
        { "kind": "group", "id": "std",
          "visible": { "view_model": "view_model_data.hud_halo_gm", "not": true, "default": true },
          "children": [
            { "kind": "sprite", "id": "boost", "sprite": "lit",
              "visible": { "view_model": "view_model_data.hud_halo_boost" } },
            { "kind": "text", "id": "speed", "style": "OSD_24_bold", "align": "centre", "box": 40,
              "cap_height": 9.5, "at": [0, -14.9], "value": { "game": "speed_str" } },
            { "kind": "text", "id": "pos", "style": "17_bold", "align": "right", "box": 14,
              "cap_height": 6, "at": [-40, 12], "value": { "session": "current_car.position" } },
            { "kind": "levels", "id": "dp", "sprite": "lit", "count": 16,
              "show": "view_model_data.hud_halo_dp{nn}" },
            { "kind": "slats", "id": "th", "sprites": ["lit", "lit", "lit"],
              "show": "view_model_data.hud_halo_th{nn}" },
            { "kind": "gauge", "id": "batt", "sprite": "fill",
              "path": [[-20, 12], [-4, 12]], "value": { "game": "ers_stored" } },
            { "kind": "sweep", "id": "rpm", "mask_sprite": "mask", "fill_sprite": "solid",
              "pivot": [0, 0], "start": 225, "arc": [[0, 0], [1, -270]],
              "value": { "game": "revs" }, "tint": { "session": "current_car.nub_colour" } }
          ] }
      ],
      "decisions": [],
      "editor": {
        "preview": { "game.revs": 0.62, "game.speed_str": "135", "game.ers_stored": 0.75,
                     "view_model_data.hud_halo_boost": true, "view_model_data.hud_halo_dp10": true },
        "team_colour": "#27F4D2"
      },
      "variants": {
        "f2": { "dial": "f2", "remove": ["batt"] }
      }
    }
  }
}
```

`swofty hud compose <addon folder> registry/hud.json` writes the archives to a folder, `swofty hud check` asserts the stock facts below against the game's original files.

## Units and placement

One HUD unit is one pixel of a 640x360 frame, the origin is the dial centre and `v` points down. A sprite's `bounds` is where it sits in that frame, so a sprite's art lines up with the others without an `at`. The record's root sits where the bottom of its box lands on the stock bottom customisation point; the box is `box`, else the bounds of the sprite named `base`, else the bounds of every sprite. Every element hangs off `boundless_customisation`, so the HUD editor's move and resize apply to it.

## Element kinds

```jsonc
{ "kind": "sprite", "id": "a", "sprite": "base", "layer": 20, "at": [0, 0], "rot": 0 }
{ "kind": "text",   "id": "b", "style": "OSD_24_bold", "align": "left|centre|right", "box": 40,
  "cap_height": 9.5, "at": [x, y], "rot": 0, "case": "upper|none", "wrap": "none|clampwidth",
  "value": { "game": "speed_str" } }              // or "literal": "BOOST"
{ "kind": "group",  "id": "c", "hidden": false, "children": [ ] }
{ "kind": "levels", "id": "d", "sprite": "lit", "count": 16, "from": 38, "to": -48,
  "show": "view_model_data.hud_x_dp{nn}" }        // one windowed copy per level, lit by one leaf each
{ "kind": "slats",  "id": "e", "sprites": ["s1", "s2"], "show": "view_model_data.hud_x_th{nn}" }
{ "kind": "gauge",  "id": "f", "sprite": "fill", "path": [[x0, y0], [x1, y1]], "window": [w, h],
  "value": { "game": "ers_stored" } }              // a window slides over the fill as the value rises
{ "kind": "sweep",  "id": "g", "mask_sprite": "m", "fill_sprite": "solid", "pivot": [0, 0],
  "start": 225, "arc": [[0, 0], [1, -270]], "value": { "game": "revs" } }
```

Text origins are the alignment anchor and the baseline; the composer places each text on a rotated, scaled null with the game's measured cap height of 0.672 em, so `cap_height` is the height of a capital in HUD units. Known styles are `17_bold`, `24_bold`, `OSD_17_bold` and `OSD_24_bold`.

`{nn}` in a `show` pattern is the two-digit index, from 01. A `levels` window `i` shows the lower part of the sprite up to `from + (to - from) * i / count`. `arc` is `[fraction, degrees]` keys of the sweep turning about `pivot`, relative to the frame turned to `start` degrees.

## Bindings

A binding names exactly one source:

```jsonc
{ "view_model": "view_model_data.hud_halo_boost" }   // your own leaf, written with hud::set
{ "game": "revs" }                                   // a value of the dial's own view model
{ "session": "current_car.position" }                // a #session path
{ "session_car": "position", "slot": 5 }             // #session.cars.car5.position
```

`"not": true` negates a bool and `"default"` is the value while the binding is absent. A leaf's type comes from where it is bound: a bool for `visible`, text for a text, a float for a gauge or sweep (`"ty": "i32"` binds a number as text). The game values and their tags:

| `game` | tag | use |
|---|---|---|
| `speed_str` | text | text |
| `revs`, `ers_stored` | float | gauge, sweep |
| `om_active`, `is_xm_active`, `has_ers` | bool | visible |
| `ers_mode` | ui32 | decision only |
| `drs_pit_state` | MVCD | decision only |

`session` takes `current_car.position` (ui32, text), `current_car.driver.second_name` (text) and `current_car.nub_colour` (colour, `tint`). `session_car` takes `position` and `driver.second_name`. A property's tag must be the tag of the value it binds: the game takes it on trust, and a text bound to the ui32 position crashed the game while it created the HUD. The build refuses a mismatch, and checks every game binding against the tags the stock template binds.

## Decisions

An element a decision lists is shown by the decision and has no `visible` of its own:

```jsonc
"elements": [
  { "kind": "sprite", "id": "drs_open", "sprite": "drs_open" },
  { "kind": "sprite", "id": "drs_off",  "sprite": "drs_off" },
  { "kind": "sprite", "id": "ers_hotlap", "sprite": "ers_hotlap" }
],
"decisions": [
  { "id": "drs", "type": "mvcd", "game": "drs_pit_state",
    "states": { "drs_live": ["drs_open"], "invalid": ["drs_off"] } },       // other states hide every listed element
  { "id": "mode", "type": "svcd", "game": "ers_mode", "equals": 2, "ty": "ui32",
    "yes": ["ers_hotlap"], "no": [] }
]
```

The composer writes an `MVCD` or `SVCD` node under the record's template group and registers and queries it from the dial's own `show` command, unregistering it from `hide`, as the stock DRS widget does. An `mvcd` takes an event for every state of the stock `MVCD` on the same value under the same dial; a state the record doesn't list hides everything.

## Stock parts, variants and the editor

`stock` decides what happens to the dial's `frame`, `fuel`, `holder_central` and `pit_indicator` (or `stock_parts`): `park` moves them 60 units off screen and shrinks them, `hide` has the dial template set their `visible` to false, `keep` leaves them. A variant is a patch of the record for another dial; its `switch` binding shows the record while the leaf is on and the stock parts while it is off:

```jsonc
"variants": {
  "f1_25": { "dial": "f1_25", "remove": ["boost"], "switch": { "view_model": "view_model_data.hud_halo_cls", "default": true } },
  "f2":    { "dial": "f2", "remove": ["batt"] }
}
```

The HUD editor shows `dial_complete.idf` with no template running: the scene's own state plus its `.uipre` setters. So each record also gets a static copy of the elements that `editor.preview` shows (keys: `game.<name>`, `session.<path>`, `session_car.<field>.<slot>` or the leaf), its sprites pre-tinted with `team_colour`, `.uipre` setters that hide the stock parts and fill the copy's texts, and a `<prefix>_pv_ctl` group that hides the copy in game. The spectator HUD's dial of each class runs the same template groups unless the record sets `"spectator": false`.

## A scene target

A record that grows one of the dials names `dial`. A record that adds readouts to some other stock scene names that scene and the template node that instances it instead:

```jsonc [registry/damage_readouts.json]
{
  "hud.element": {
    "readouts": {
      "scene": "eaid://ui_package/art/osd/components/dial/mfd_car_damage_panel.idf",
      "parent_ui": "hud_mfd_dynamic.mfd.attacher.car_damage_panel",
      "elements": [
        { "kind": "text", "id": "brake_fl", "style": "OSD_17_bold", "align": "left", "box": 30,
          "cap_height": 6, "at": [-60, 40],
          "value":   { "view_model": "ui_menu_data.hud_brake_check_fl" },
          "visible": { "view_model": "ui_menu_data.hud_brake_check_visible" } },
        { "kind": "text", "id": "plank", "cap_height": 6, "at": [0, 50],
          "value": { "view_model": "ui_menu_data.hud_planked_text" } }
      ]
    }
  }
}
```

`scene` is the `eaid://` path of a stock `.idf`, read from `flow_race.erp`. `parent_ui` is the dotted path in the template (root name first) of the node whose component is that scene; the build stops unless the node instances the scene. A record has a `dial` or a `scene` and `parent_ui`, never both.

The core adds the elements under the scene's root element, in HUD units from the scene's origin (`v` down, as in the dials), in a null named for the record's prefix, and one bound node per text or visibility straight under `parent_ui`, named `<prefix>_<name>` (`t_<id>` for a text, `v_<id>` for a group's visibility). There is no `<prefix>_ctl` group and no `show` or `hide` wiring, because the parent is no dial. The scene's other elements and the parent's other children are not touched.

A scene target takes `text` and `group` elements with `value`, `literal`, `visible`, `at`, `style`, `align`, `box`, `case`, `wrap` and `cap_height`. It takes no sprites, decisions, variants, `stock`, `editor` or `box` on the record, and no `layer` or `tint`. Binding types are held to the stock template exactly as for the dials: a game or session value must have the tag the property reads, and a leaf has the type its use gives it. A leaf may live in `view_model_data` or in `ui_menu_data`; the second is the dictionary some addons already write their values to, so their readouts keep their paths.

The scene a scene target grows is read after every other addon's whole replacement of it has been merged ([two addons replacing one UI resource](/reference/ops/erp#two-addons-replacing-one-ui-resource)), so a scene target and an addon that ships its own copy of the scene stay together. The archives are `hud_target_scene.erp` (the grown scene, overlaid on the archives that carry both the template and the scene) and `hud_template.erp`.

## Bridge addons

Two addons that each replace `hud_dynamic_templates.ui` whole used to leave the later one the winner, and a third addon spliced their copies together by hand (Project Halo + Planked + Brake Check Panel does that). The build now merges whole replacements of the template and of the scenes node by node, and a `hud.element` composition starts from the merged result, so that bridge is no longer needed. See [two addons replacing one UI resource](/reference/ops/erp#two-addons-replacing-one-ui-resource) for the rules and the build lines and warnings.

## What the composer writes

One scene growth per dial and one template pass, whatever the number of addons:

```text
hud_scene.erp      the grown dial scenes, anim clips and string tables, the editor .uipre, the textures
                   overlaid on flow_race, flow_options, persistent and screens/hud_spectator_2
hud_template.erp   hud_dynamic_templates.ui with a <prefix>_ctl group under each dial
                   overlaid on flow_race and flow_options
hud_spectator.erp  hud_spectator_2.ui with the same groups under its three dials
                   overlaid on flow_race and screens/hud_spectator_2
```

Every element and template node name starts with the record's `prefix` (default: the addon id and the record key, lowercase, `_` between words, at most 24 characters), so two addons never share a name. The archives are ready `overlay` operations, so the runtime loader takes the grown scenes as they are.

## Composition order

The composer is the last step that writes these resources, and it builds on what the others leave:

```text
1. the game's original archives            read through the launcher's baseline, never a modified folder
2. addons' own archive operations          replace-resource, put-resource, patch-pkg and overlay on the
                                           template, the spectator file, the dial scenes, their string
                                           tables and .uipre files, applied in load order to a scratch copy
3. the ADDONS MFD page                     the launcher's own edit of hud_dynamic_templates.ui, from the
                                           options of addons that declare `hud.mfd`
4. the composer                            grows the scenes and adds its nodes to the result of 1 to 3
5. the overlay operations                  the composed archives go in after every other addon's
                                           operations, under their own origin ("<addon> (HUD composer)")
```

When two or more addons replace the same template or scene whole in step 2, the scratch copy holds their [node by node merge](/reference/ops/erp#two-addons-replacing-one-ui-resource), not the last one.

So the one template that ends up in the game carries every addon's `<prefix>_ctl` group, the MFD page, and the edit of an addon that replaced `hud_dynamic_templates.ui` (Planked Brake Panel's `replace-resource`, say). Step 3 is applied again by the menu build on the data route; it does nothing the second time, so the page is in the template once. On the runtime route the composed overlay archive already holds the page, so the overlay does not take it away.

The build's `hud:` lines say which addon's change the composed archives were built on top of.

## When another addon's change cannot be carried

A change from step 2 that fails to apply to the scratch copy (a `patch-pkg` that no longer finds its node under the composed dial, say) is not carried. The build goes on and warns, naming both addons:

```text
Planked Brake Panel: its change to a HUD resource cannot be applied under Halo's composed HUD (<reason>); the composed overlay replaces that resource with a copy of the game's own
```

Two limits:

- The composer reads `flow_race.erp`'s copy of each resource. A change that only another archive carries (`flow_options.erp`'s copy of the template) is not seen.
- A whole-archive `put` of `flow_race.erp`, `flow_options.erp`, `persistent.erp` or `screens/hud_spectator_2.erp` is read as that archive, and a later put of one of them drops the composed overlay for it like any other overlay.

A compose error (an unknown game value, a missing sprite, a prefix two records share) stops the build with `the HUD elements do not compose: <reason>`.

## Rules the composer keeps

- `isHidden` on an element does not hide it in game or in the editor; only a template node's `visible` does. A root hidden by default has a `visible` node, and the spectator HUD gets its own groups.
- Stock element ids never move, element names are unique in the scene, and every id is the djb2 hash of its dotted path; the build stops otherwise.
- Every bound property has the tag of its value; decision nodes read the values the stock dial reads.
- Everything hangs off `boundless_customisation`; the selection box (`tl_customisation`, `br_customisation`) is the union of the records' boxes.
- One scene is stored in several archives; the composed archives go to all of them.
- A record's nodes are checked against the stock files: the template without the added groups and the signals to them is the stock template, node for node.

The anim nodes a gauge or sweep adds are named `zh_<element>...`, which sorts after every stock name in the scene's anim string table.

::: info Credit
The element templates, the placement maths, the clip keys and the binding rules follow Project Halo by nyannoying (https://github.com/nyannoying1337/f1-25-project-halo). Uses work from Project Halo by nyannoying.
:::
