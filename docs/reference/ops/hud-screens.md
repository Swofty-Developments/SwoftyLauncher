---
title: HUD screens
description: Put hud.element records on the standings board, the replay screen and the spectator tower with screen, show and also; hide the game's own parts with stock_nodes; restyle the Race Control banner with restyle.
---

# HUD screens

A `hud.element` record with `screen` instead of `dial` puts its elements on one of the game's own
boards: the standings tower on the HUD, the replay screen's drivers list or the spectator screen's
tower. The elements are built once into the board's bottom cap scene; every screen the record names
gets its own instance of that cap and a control group in its template that shows the instance and
masks the stock board while the record's `show` leaf is true. While the leaf is false or absent (no
DLL), the stock board is the game's own.

```json [swofty.json]
"operations": {
  "hud": ["registry/tower.json"]
}
```

## One tower on all three screens

```jsonc [registry/tower.json]
{
  "hud.element": {
    "tower": {
      "screen": "standings",                                           // the HUD standings board
      "show": { "view_model": "view_model_data.hud_tt_show" },         // host visible <- show; masks <- NOT show
      "also": {                                                        // the same elements on more screens
        "replay":    { "show": { "view_model": "view_model_data.hud_tt_on" } },
        "spectator": { "show": { "view_model": "view_model_data.hud_tt_on" } }
      },
      "sprites": {
        "strip": { "png": "art/strip.png", "bounds": [0, 0, 300, 21.6] },  // HUD units from the tower's top-left
        "me":    { "png": "art/me.png",    "bounds": [0, 0, 300, 21.6] },
        "bn_m":  { "png": "art/bn_m.png",  "bounds": [0, 0, 8, 8] }        // the banner's navy fill
      },
      "elements": [
        { "kind": "group", "id": "r00", "visible": { "view_model": "view_model_data.hud_tt_r00_on" },
          "children": [
            { "kind": "sprite", "id": "s00", "sprite": "strip", "layer": 20 },
            { "kind": "sprite", "id": "me00", "sprite": "me", "layer": 22,
              "visible": { "view_model": "view_model_data.hud_tt_r00_me" } },
            { "kind": "text", "id": "pos00", "style": "OSD_24_bold", "align": "centre", "box": 15, "cap_height": 9.8,
              "at": [8.8, 14.6], "value": { "view_model": "view_model_data.hud_tt_r00_pos" } },
            { "kind": "text", "id": "code00", "style": "OSD_24_bold", "align": "left", "box": 40, "cap_height": 12.5,
              "wrap": "clampwidth", "at": [23.7, 14.6], "value": { "session_car": "driver.second_name", "slot": 0 } }
          ] }
      ],
      "stock_nodes": [
        { "template": "hud_lap_counter.osd_visibility_option",
          "hide": ["car_category_logo", "edge_base", "edge_border", "lap_bg", "lockup", "session_icon",
                   "centre_lap_value", "separator_bg", "separator_bg_top", "centre_formation"],
          "while": { "view_model": "view_model_data.hud_tt_show" } }
      ],
      "restyle": { "preset": "banner", "sprite": "bn_m" }
    }
  }
}
```

Sprites are quads of the cap's own `pill_base` mesh with one material binding per sprite, so they
draw in every instance. Coordinates are HUD units from the cap's origin (the board's top-left), `y`
down, as in a dial record; `visible`, `value`, `tint`, `levels`, `slats` and the rest work as they
do in a dial. A screen record takes no `decisions`, `variants`, `stock` or `stock_parts`.

What one record composes:

| archive | into | holds |
|---|---|---|
| `hud_screen_scene.erp` | flow_race, flow_options, persistent | the grown cap, the standings table with the `<prefix>_host` instance, the table's `.uipre` (stock board hidden in the HUD editor), the sprites, the restyled scenes |
| `hud_template.erp` | flow_race, flow_options | `hud_dynamic_templates.ui` with `<prefix>_ctl` under `hud_track_position.osd_visibility_option.track_position`, the `stock_nodes` groups and the restyle's static hides |
| `hud_spectator.erp` | flow_race, screens/hud_spectator_2 | `hud_spectator_2.ui` with `<prefix>_ctl` under `hud_spectator_2.content`, and the spectator table scene with its `<prefix>_host` |
| `hud_replay.erp` | flow_replay, screens/replays_and_highlights | `replays_and_highlights.ui` with `<prefix>_ctl` under the screen root, and the replay scene with its `<prefix>_host` |
| `hud_replay_scene.erp` | flow_frontend | the replay scene again (that archive has the scene without the template) |

## The control group

Every screen gets the same shape under its parent node, so what a leaf does on the HUD it does on
the replay and spectator screens too:

```text
hud_track_position.osd_visibility_option.track_position
  <prefix>_ctl                      (UIGP; trigger/untrigger wired to the parent's enable/disable)
    mask_0        mask              visible <- NOT show, default true   (the stock board)
    host_n        <prefix>_host     visible <- show, default false
      v_cap       pill_rotation_anim   visible = false  (the cap's own graphics)
      root_n      <prefix>            visible = true   (replay: <- show_drivers_list; spectator: <- show_left_side)
      v_r00       <prefix>_r00         visible <- hud_tt_r00_on
      t_pos00     <prefix>_pos00       text    <- hud_tt_r00_pos (stri)
      ...
    extra_hide    <prefix>_host > Scene Root   (replay only) visible <- NOT is_video_sting_enabled
  bottom_cap
    <prefix>_sn1  v_<prefix>  visible = false   (the stock cap instance never shows the elements)
```

Masks on the other screens are layout chains into the stock instances: replay `driver_table > Scene
Root` and `session_timer > Scene Root`; spectator `spectator_mode_2 > driver_position_table >
transition` and `spectator_mode_2 > session_timer > Scene Root`. Every binding is typed against the
stock template of its screen (a wrong tag crashes the game when the screen opens), and the stock
nodes are left byte for byte as they were.

## The three presets

| | `standings` | `replay` | `spectator` |
|---|---|---|---|
| template, parent | `templates/hud_dynamic_templates.ui`, `hud_track_position.osd_visibility_option.track_position` | `screens/replays_and_highlights.ui`, `replays_and_highlights` | `screens/hud_spectator_2.ui`, `hud_spectator_2.content` |
| instance | clone of `driver_position_bottom_cap` in `driver_position_table_stacked.idf`, at (0, 0), scale 1 | clone of `driver_table` in `theatre/replay_highlights.idf`, retargeted to the cap, at `session_timer`'s position, scale 0.78 | clone of `bottom_cap` in `spectator_mode/spec_driver_position_table.idf`, retargeted to the cap, offset header minus table from `spectator_mode_2.idf`, scale 0.78 |
| root follows | — | `#replay_flow.replays_and_highlights.show_drivers_list` | `show_left_side` |
| stock read from | flow_race.erp | flow_replay.erp | flow_race.erp |

`spectator_mode_2.idf` itself is left stock: another addon replaces it, and the later wins.

## A screen spelled out

For a screen the core does not know, `screen` takes the same struct the presets fill:

```jsonc
"screen": {
  "key": "my_screen",
  "template": "eaid://ui_package/screens/my_screen.ui",
  "parent_ui": "my_screen.content",
  "host": { "scene": "eaid://ui_package/art/osd/units/driver_position_table/driver_position_bottom_cap.idf",
            "mesh": "pill_base", "null": "driver_group" },
  "instance": { "in": "eaid://ui_package/art/osd/screens/my_screen.idf", "clone": "some_xref",
                "under": "elements_null", "at": { "of": "session_timer" }, "scale": 0.78, "retarget": true },
  "masks": [["some_xref", "Scene Root"]],
  "root_switch": "show_left_side",
  "erps": { "scene": ["2025_asset_groups/ui_package/flow_frontend.erp"],
            "template": ["2025_asset_groups/ui_package/screens/my_screen.erp"] },
  "stock_erp": "2025_asset_groups/ui_package/flow_race.erp",
  "template_erp": "2025_asset_groups/ui_package/flow_race.erp",
  "scene_file": "hud_my_screen_scene.erp",
  "template_file": "hud_my_screen.erp"
}
```

`at` is `{ "point": [x, y] }` in scene units, `{ "of": "element" }` for another element's position,
or `{ "offset": { "scene": "…idf", "a": "header", "b": "table" } }` for `a` minus `b` read from
another stock scene. `wire` is `["enable", "disable"]` when the parent has those commands.

## `stock_nodes`: hide the game's own parts

```jsonc
"stock_nodes": [
  { "template": "hud_lap_counter.osd_visibility_option",          // a dotted template path
    "hide": ["lap_bg", "lockup", "centre_lap_value"],              // elements of the scene that node instances
    "while": { "view_model": "view_model_data.hud_tt_show" } },    // visible <- NOT while, default true
  { "template": "hud_track_position.osd_visibility_option.track_position.bottom_cap",
    "scene": "eaid://ui_package/art/osd/units/driver_position_table/driver_position_bottom_cap.idf",
    "hide": ["pill_rotation_anim"] }                               // no while: a static visible = false
]
```

One group `<prefix>_sn<i>` per entry goes under the template node, with a `v_<element>` node per
element. The scene is the one the template node (or its nearest ancestor) instances; name it with
`scene` when the node has no component. The composer refuses an element that is not in the scene
exactly once, is already `isHidden`, or that a stock node under that template path already shows or
hides: a second `visible` binding on one element fights the game's own. With the leaf absent the
default `true` keeps the game's part visible, so the stock HUD is whole without the DLL.

## `restyle`: the Race Control banner

The preset is Project Halo's banner: the twelve toast scenes under `art/osd/{components,units}/message_system`
(and `illegal_overtake`) restyled in place with no element added, moved or renamed.

```jsonc
"restyle": { "preset": "banner", "sprite": "bn_m" }
```

It expands to one entry per scene; the long form is the same shape, so a record can restyle any scene:

```jsonc
"restyle": [
  { "scene": "eaid://ui_package/art/osd/components/message_system/message_system.idf",
    "scale": [ { "element": "centre", "by": 0.7674 }, { "element": "safety_car", "by": 0.7674 },
               { "element": "red_flag", "by": 0.7674 }, { "element": "safety_rating", "by": 0.7674 } ] },
  { "scene": "eaid://ui_package/art/osd/units/message_system/central_message.idf",
    "fill": { "pill": { "sprite": "bn_m", "slot": "central_message_bg" } },   // the fill's geom materialid <- one shared .emb
    "twins": ["pill"],                                                        // a clip-free copy pill_bn at layer 60
    "hide": ["edge_border", "edge_base", "ms_colour"],                        // isHidden + static visible=false nodes
    "template_parents": [
      "hud_notifications_dynamic.osd_visibility_option.hud_notifications_dynamic.notification_0.hud_message.right.central_message",
      "hud_notifications_dynamic_p2.osd_visibility_option.hud_notifications_dynamic_p2.notification_0.hud_message.right.central_message" ],
    "text": { "text_line_1": { "style": "OSD_34_bold" }, "text_line_2": { "style": "34_regular" } },
    "params": [ { "element": "stacker_content", "name": "stackerType", "value": "l" } ] },
  { "scene": "eaid://ui_package/art/osd/units/message_system/race_control_icon.idf",
    "fill": { "icon": { "sprite": "bn_m", "slot": "race_control_icon" } }, "twins": ["icon"],
    "hide": ["edge_border", "edge_base", "ms_colour", "bg_frame"],
    "template_parents": [ "…left.race_control_icon", "…left.race_control_icon" ],
    "text": { "text": { "style": "OSD_34_bold", "box": 3.2 } } }
]
```

- `fill`: every fill of one sprite points at one shared binding
  `art/osd/components/hud/<prefix>_restyle_<sprite>.emb` that maps each listed mesh slot to the
  sprite's material. The slot must be a slot of that quad's own mesh (checked in the stock mesh).
- `twins`: a fill the toast animates through a clip at an offset layer drew nothing in game, so it
  gets a copy `<element>_bn` beside it with the same mesh and binding, no clip, absolute layer 60.
- `hide`: `isHidden` on the element and a static `<prefix>_hide_<element>` node under every
  `template_parents` path, in both toast instances. `isHidden` alone did not hold in game.
- `scale`: `position.y = pivot_y + (y - pivot_y) * by`, `scale *= by` (x and y); `pivot_y`
  defaults to the element's own `y` (scale in place).
- `text`: `style`, `box`, `align`, `case`, `at` on an eText; `params`: any bool, number (fp32) or
  string param.

The RACE CONTROL label's clip colour (its `text?context=animation` DiffuseColour keys) is not
rewritten yet; the label keeps the stock navy on the navy fill.

## Checks

`swofty hud check` asserts every fact above against the game's original files: the cap's
`pill_base` mesh (3.04 x 0.12) and `driver_group`; the stacked table's `driver_position_bottom_cap`
reference, `mask` and top-down `full_stacker`; `track_position`'s one `enable` and one `disable`
and its childless `bottom_cap`; the lap counter's eight top-level parts and its ten unbound,
unhidden parts; the replay screen root (UISC), its `show_drivers_list` and `is_video_sting_enabled`
bools, the board references at scale 0.9 and (-9.2, 5.0); the spectator `content` group,
`show_left_side`, the table scene's `transition` and `bottom_cap`, the screen scene's two references
at scale 1; the twelve toast scenes' fill slots, offset layers, decorations and both toast groups.

## Not yet

The HUD editor's 5-row preview copy inside the instanced host (nested `.uipre` setters), decisions on
a screen target, `stock_nodes` on the replay template and the series logo `texture` binding per
screen land with the rows, units and textures work.
