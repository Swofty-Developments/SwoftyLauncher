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
        "replay":    { "show": { "view_model": "view_model_data.hud_tt_on" }, "hide": ["trial"] },   // hide: element ids kept off this screen
        "spectator": { "show": { "view_model": "view_model_data.hud_tt_on" }, "hide": ["trial"] }
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
          "scene": "eaid://ui_package/art/osd/components/lap_counter/lap_counter.idf",
          "hide": ["car_category_logo", "session_icon", "centre_lap_value", "separator_bg", "separator_bg_top", "centre_formation"],
          "wrap": ["edge_base", "edge_border", "lap_bg", "lockup"],    // parts a stock node targets: a new null around each, hidden instead
          "while": { "view_model": "view_model_data.hud_tt_show" } },
        { "template": "hud_left_notification_dynamic.osd_visibility_option.left_notification.left_message",
          "hide": ["boundless"], "while": { "view_model": "view_model_data.hud_tt_show" } }   // the flag box
      ],
      "restyle": { "preset": "banner", "sprite": "bn_m" }
    }
  }
}
```

Sprites are quads of the cap's own `pill_base` mesh with one material binding per sprite, so they
draw in every instance. Coordinates are HUD units from the cap's origin (the board's top-left), `y`
down, as in a dial record; `visible`, `value`, `tint`, `levels`, `slats` and the rest work as they
do in a dial, and so do `unit` elements, `texture` bindings and stock-texture sprites
([HUD textures and units](/reference/ops/hud-textures)): a `{ "screen": "series_logo" }` texture reads
the standings' lap counter leaf on the HUD, `#replay_flow.replays_and_highlights.logo_texture` on the
replay screen and the spectator view model's `logo_texture` there. A text that reads
`#session.time_remaining_str` gets the stock session timer's formatter cloned into the control group
(see [the session clock](#the-session-clock)). A screen record takes no `variants`, `stock` or
`stock_parts`; its `decisions` run `on` the game's own paths (see
[decisions on a screen](#decisions-on-a-screen)).

What one record composes:

| archive | into | holds |
|---|---|---|
| `hud_screen_scene.erp` | flow_race, flow_options, persistent | the record's own host scene `driver_position_bottom_cap_<prefix>.idf` (a copy of the cap with the live root `<prefix>` and the editor copy `<prefix>_pv`; the stock cap stays as the game has it), the standings table with the `<prefix>_host` instance of it, the table's `.uipre` (stock board hidden in the HUD editor, the cap's graphics and the live root hidden inside the host, the copy shown and its texts filled), the sprites, the restyled scenes |
| `hud_template.erp` | flow_race, flow_options | `hud_dynamic_templates.ui` with `<prefix>_ctl` under `hud_track_position.osd_visibility_option.track_position`, the `stock_nodes` groups and the restyle's static hides |
| `hud_spectator.erp` | flow_race, screens/hud_spectator_2 | `hud_spectator_2.ui` with `<prefix>_ctl` under `hud_spectator_2.content`, and the spectator table scene with its `<prefix>_host` |
| `hud_replay.erp` | flow_replay, screens/replays_and_highlights | `replays_and_highlights.ui` with `<prefix>_ctl` under the screen root, and the replay scene with its `<prefix>_host` |
| `hud_replay_scene.erp` | flow_frontend | the replay scene again (that archive has the scene without the template) |
| `hud_screen_scene_2.erp` | screens/hud, screens/hud_player_1, screens/hud_player_2 | the standings table and its `.uipre`, the cap's anim string table when a record has clips, the restyled Race Control scenes, with the host scenes, sprites and clips of `hud_screen_scene.erp`: the six archives that carry the standings table all get the composed copy |

## The control group

Every screen gets the same shape under its parent node, so what a leaf does on the HUD it does on
the replay and spectator screens too:

```text
hud_track_position.osd_visibility_option.track_position
  <prefix>_ctl                      (UIGP; trigger/untrigger wired to the parent's enable/disable)
    mask_0        mask              visible <- NOT show, default true   (the stock board)
    host_n        <prefix>_host     visible <- show, default false
      v_cap       pill_rotation_anim   visible = false  (the cap's own graphics)
      v_pv        <prefix>_pv          visible = false  (the editor copy)
      root_n      <prefix>            visible = true   (replay: <- show_drivers_list; spectator: <- show_left_side)
      h_trial     <prefix>_trial       visible = false  (also.<screen>.hide, this screen only)
      v_r00       <prefix>_r00         visible <- hud_tt_r00_on
      t_pos00     <prefix>_pos00       text    <- hud_tt_r00_pos (stri)
      c_pb00      <prefix>_pb00        colourOverride = #58d654ff   (a text's colour)
      v_notes     <prefix>_notes       visible = false  (an element with hidden: true)
      ...
    extra_hide    <prefix>_host > Scene Root   (replay only) visible <- NOT is_video_sting_enabled
    clock_fmt     UITF  #session.time_remaining -> time_remaining_str (or clock.output); formatTime <- trigger
    mv_clock      MVCD on clock.start: state 1 -> clock_fmt.formatTime
    d_tt          MVCD on a decision's path (standings only)
```

`also.<the record's own screen>` takes only `hide`: those elements stay off the record's own screen
(for one record that shows a short list on the HUD and the long one on the replay screen).

`isHidden` on a scene element does not hide it in game, so every static hide here is a template node:
`hidden: true` on an element, `also.<screen>.hide`, the cap's graphics and the editor copy.

Each screen record has a host scene of its own, `driver_position_bottom_cap_<prefix>.idf`: a copy of
the cap that holds only that record's root and editor copy, under the cap's anim, mesh and material
names. Every `<prefix>_host` instance, on every screen the record names, references it (`fileRef`
and `xref`, with the cap's material binding kept). The stock board's own cap instance draws the cap as
the game has it, so it holds no record and needs no hides, and no record's elements draw inside
another record's host.

A board holds the bare cap once and each record's host once: `(1 + records) x cap + every record`
walk slots. A family of addons that each must work alone (a season per addon, say) can carry one
shared record instead of
one each: when several addons bring a record with the same key, the same explicit `prefix`, equal
fields and the same bytes in every sprite, the build composes it once, from the first addon in the
load order, and prints `<prefix>: <addon> brings the same record <key> as <first>; composed once`
for the others. Records that share a prefix and differ in anything still stop the build.

Masks on the other screens are layout chains into the stock instances: replay `driver_table > Scene
Root` and `session_timer > Scene Root`; spectator `spectator_mode_2 > driver_position_table >
transition` and `spectator_mode_2 > session_timer > Scene Root`. Every binding is typed against the
stock template of its screen (a wrong tag crashes the game when the screen opens), and the stock
nodes are left byte for byte as they were.

### Several records on one screen

Each mask is one `visible` binding on a stock element, and two bindings on one element fight: the
record composed last wins, so the stock board shows while the other record is on. With two or more
records on a screen, the first record by prefix keeps the stock chains. Each later record wraps the
masked element in a null of its own, `<prefix>_w_<element>`, and its masks bind that null:

| screen | mask | the later record's wrapper | in |
|---|---|---|---|
| `standings` | `mask` | `<prefix>_w_mask` | `driver_position_table_stacked.idf` |
| `replay` | `driver_table > Scene Root` | `<prefix>_w_driver_table` | `theatre/replay_highlights.idf` |
| `replay` | `session_timer > Scene Root` | `<prefix>_w_session_timer` | `theatre/replay_highlights.idf` |
| `spectator` | `… > transition` | `spectator_mode_2 > driver_position_table > <prefix>_w_transition` | `spectator_mode/spec_driver_position_table.idf` |
| `spectator` | `… > session_timer > Scene Root` | `spectator_mode_2 > <prefix>_w_session_timer` | `screens/spectator_mode_2.idf` |

A third record's null goes inside the second's, as for `stock_nodes`. Every record's mask hides the
board while its own `show` leaf is true, and the board shows only while every leaf is false. The
composer writes these scenes only with two or more records on the screen. A screen spelled out
names each mask's scene in `mask_scenes`; without it, the wrappers go into the instance scene.

The host instances need nothing of this: each record's `<prefix>_host` instances its own host scene.

### How big a board can grow

The game walks a screen's UI tree as one flat list of 16-byte slots with 16-bit child indices, so one
tree holds at most 65,536 slots. A board holds the stock cap once and the host scene of each screen
record on the screen once, and each host holds the cap and its own record's root and editor copy:

```text
board      = (1 + records on the screen) x cap slots + every record's root and editor copy
race tree  = board + about 17,500 slots of the rest of the race HUD
HUD editor = 3 x board + 816
```

SWOFTY 2010, SWOFTY 2011 and TV Timing Tower on the standings make a 14,125-slot board, a race tree
of about 31,625 slots and a HUD editor tree of about 43,191. A board past the limit stops the build
before the game starts, naming every addon with a record on the screen and the ones whose removal
alone brings the tree inside the limit. Three addons with two 9,510-slot towers each make a
57,116-slot board and a race tree of about 74,616:

```text
the HUD elements do not compose: Tower A, Tower B and Tower C together make the Track Position list too big for the game: about 74616 of the 65536 slots one screen can hold, and the game can close as soon as it shows it; turn Tower A, Tower B or Tower C off in this profile (the board holds 7 host scenes, 57116 walk slots in all)
```

A standings board whose HUD editor tree passes the limit while the race tree fits still builds, and
the race HUD is unchanged. The build gives a warning that names every addon with a record on the
standings and the ones whose removal alone brings the HUD editor tree inside the limit. Three
9,510-slot towers make a 28,562-slot board, a race tree of about 46,062 slots and a HUD editor tree
of about 86,502:

```text
Tower A, Tower B and Tower C together make the Track Position list too big for HUD customisation (OSD Customization in the pause menu): about 86502 of the 65536 slots one screen can hold, and the game closes when it opens. Races show the list as before. Turn Tower A, Tower B or Tower C off in this profile before you open HUD customisation (it holds the board 3 times, 4 host scenes of 28562 walk slots each time)
```

The warning is in the build log and in the profile's conflict report (`hud_editor` in
`conflicts.json`). The launcher shows it as a warning that stays until it is closed: on Play, when
the profile's last build had it and the profile still turns on the same addons in the same order,
and otherwise once the game starts after the build that found it.

The HUD editor tree cannot be made smaller on its own: HUD customisation draws
`driver_position_table_stacked.idf` and its `.uipre`, the race HUD instances the same scene, and the
game keeps one copy of a resource, from whichever archive it reads first. The `.uipre` only sets
`Visible` and text, and the flatten counts hidden nodes too.

The build lists each record's nodes, walk slots and walk stack (256 at most), and each screen's
board and tree; the standings line also gives the HUD editor tree.

## The three presets

| | `standings` | `replay` | `spectator` |
|---|---|---|---|
| template, parent | `templates/hud_dynamic_templates.ui`, `hud_track_position.osd_visibility_option.track_position` | `screens/replays_and_highlights.ui`, `replays_and_highlights` | `screens/hud_spectator_2.ui`, `hud_spectator_2.content` |
| instance | clone of `driver_position_bottom_cap` in `driver_position_table_stacked.idf`, at (0, 0), scale 1 | clone of `driver_table` in `theatre/replay_highlights.idf`, retargeted to the cap, at `session_timer`'s position, scale 0.78 | clone of `bottom_cap` in `spectator_mode/spec_driver_position_table.idf`, retargeted to the cap, offset header minus table from `spectator_mode_2.idf`, scale 0.78 |
| root follows | — | `#replay_flow.replays_and_highlights.show_drivers_list` | `show_left_side` |
| stock read from | flow_race.erp | flow_replay.erp | flow_race.erp |

`spectator_mode_2.idf` itself is left stock while one record is on the spectator screen: another
addon replaces it, and the later wins. With two or more, the composer writes it for the second
record's `session_timer` wrapper.

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
  "mask_scenes": ["eaid://ui_package/art/osd/screens/my_screen.idf"],  // the scene of each mask's element
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

A part another stock node already targets (the lap counter's `edge_base`, `edge_border`, `lap_bg`
and `lockup` each have a stock group over them, so a `visible` of ours on them does not hold) goes in
`wrap` instead:

```jsonc
{ "template": "hud_lap_counter.osd_visibility_option",
  "scene": "eaid://ui_package/art/osd/components/lap_counter/lap_counter.idf",   // wrap rewrites the scene, so name it
  "hide": ["car_category_logo", "session_icon", "centre_lap_value", "separator_bg", "separator_bg_top", "centre_formation"],
  "wrap": ["edge_base", "edge_border", "lap_bg", "lockup"],
  "while": { "view_model": "view_model_data.hud_tt_show" } }
```

The composer writes the scene again with a new null `<prefix>_w_<element>` in each wrapped
element's place in its parent's children, the element inside it: every stock name stays, the
drawing order stays, the lookup grows by the wrappers, and the stock nodes still find their parts
by name. The wrapper is a copy of one of the scene's plain nulls, with no art of its own. The group
then hides the wrappers (`v_w_<element>`, `visible <- NOT while` like the rest). The flag box
(`boundless` in `race_condition_flag_message.idf`, under
`hud_left_notification_dynamic.osd_visibility_option.left_notification.left_message`) needs no wrap.

Several records can hide parts of one scene, such as two season HUDs that each show with their own
cars. The composer takes the records in the order of their prefixes, and writes each scene once:

- **A scene an earlier record wrapped:** the later record's null goes inside the earlier one,
  around the element: `s10_tower_w_lap_bg.s11_tower_w_lap_bg.lap_bg`.
- **A part an earlier record already hides:** it moves to the later record's `wrap`. Two
  `visible` bindings on one element fight, as they do with a stock node.

Each record's group then shows and hides only its own parts and wrappers.

## The session clock

A text whose `value` is `{ "session": "time_remaining_str" }` makes the control group clone the
stock session timer's formatter (`hud_session_timer_dynamic.session_timer.time_remaining_formatter`,
a UITF reading `#session.time_remaining`) as `clock_fmt`, and the group's `trigger` command signals
its `formatTime`, as the stock timer's `visible` event does. The record's `clock` gives the clone a
leaf of its own to write and a second start:

```jsonc
"clock": {
  "output": "view_model_data.hud_tt_clock",            // database_output <- this leaf; the texts read it (stri)
  "start": { "view_model": "view_model_data.hud_tt_timed" }   // an MVCD mv_clock: state 1 -> clock_fmt.formatTime
}
```

Without `output` the clone writes the stock `time_remaining_str` under the group's own view-model
scope, as the dial records do.

## Decisions on a screen

A screen record's `decisions` run `on` a path or leaf (never a dial `game` value): an `mvcd` lists
its states (`0` and `1` for a bool, as the stock pills' `active` decisions do), an `svcd` its `ty`
and `equals`. The elements a decision lists get a `v_<id>` switch node (hidden until the decision
shows them), and the decision node is registered and queried from the group's `trigger`, so it
runs on screens whose parent wires `trigger` (the standings' `enable`); on the replay and spectator
screens the listed elements stay hidden, and `also.<screen>.hide` says so in the record.

```jsonc
"decisions": [
  { "id": "tt", "type": "mvcd", "on": { "path": "#view_model_data.hud.content.lap_times.pill_0.active" },
    "states": { "1": ["trial"], "0": [] } }
]
```

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

## The HUD editor

The example addon `templates/hud-tower` is a 30-row tower with every field on this page.

The HUD customisation menu draws the standings table with no template running, so the record's live
root never shows there. The composer builds a second copy of the elements, `<prefix>_pv`, from
`editor.preview` (keyed by the substituted leaf names, as for a dial) with a top-level `repeat` cut to
`editor.rows` copies, and writes the table's `.uipre` with nested setters under the host instance:
`mask` hidden at the top level; inside `<prefix>_host` the cap's `pill_rotation_anim` and `<prefix>`
hidden, `<prefix>_pv` shown and each copy's text filled. In game `host_n` holds a static `v_pv` node,
so the copy never draws outside the editor; the stock cap instance holds no record. `unit` elements and `tint` bindings are left out of the copy.

The `.uipre` starts from the copy other addons' archive changes leave. A setter that copy already has
with the same value (`mask` hidden by another addon's own copy) is kept once; one with another value
stops the build and names both addons. Another addon's own tower on the same board stops it too:
[when another addon claims the same part](/reference/ops/hud#when-another-addon-claims-the-same-part).

```jsonc
"editor": {
  "rows": 5,
  "preview": {
    "view_model_data.hud_tt_hdr": true, "view_model_data.hud_tt_lap": "10", "view_model_data.hud_tt_laps": "71",
    "view_model_data.hud_tt_r00_on": true, "view_model_data.hud_tt_r00_pos": "10", "view_model_data.hud_tt_r00_c00": true,
    "view_model_data.hud_tt_r01_on": true, "view_model_data.hud_tt_r01_pos": "11", "view_model_data.hud_tt_r01_c01": true,
    "view_model_data.hud_tt_r02_on": true, "view_model_data.hud_tt_r02_pos": "12", "view_model_data.hud_tt_r02_me": true, "view_model_data.hud_tt_r02_c02": true,
    "view_model_data.hud_tt_r03_on": true, "view_model_data.hud_tt_r03_pos": "13", "view_model_data.hud_tt_r03_c03": true,
    "view_model_data.hud_tt_r04_on": true, "view_model_data.hud_tt_r04_pos": "14", "view_model_data.hud_tt_r04_c04": true
  }
}
```

## Not yet

`stock_nodes` on the replay template, decisions on the replay and spectator screens (their parents
register none) and the Race Control label's own colour clip (Halo rewrites its `DiffuseColour` keys
to white; the restyle leaves the clip as it is, so that label keeps the stock navy on the navy fill)
are not in the composer; each is refused or left stock with a line that says so.
