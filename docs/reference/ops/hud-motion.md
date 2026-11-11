---
title: HUD motion
description: Timed slides and fades on hud.element groups, played by the game's own animation nodes and started by a bool leaf of the addon's view model.
---

# HUD motion

A `group` in a `hud.element` record can carry one `motion`: a slide (position keys over time) or a fade (alpha from one value to another), timed by one of the game's own animation curves and started by a bool leaf the addon's DLL writes.

## A row group that slides down when the DLL says so

```jsonc [registry/tower.json]
{
  "hud.element": {
    "tower": {
      "dial": "f1_26",
      "box": [0, 0, 120, 160],
      "elements": [
        { "kind": "group", "id": "rows", "at": [0, 24],
          "motion": {
            "id": "swap",                                     // names the holder, the player and the trigger
            "keys": [[0, [0, 0]], [0.75, [0, -12.98]]],       // time in seconds, offset in HUD units from the group's own place
            "curve": "generic_fade_on",                       // a curve the stock UI templates play
            "play_on": { "view_model": "view_model_data.hud_tt_swap" }   // true plays it, false plays it back
          },
          "children": [
            { "kind": "group", "id": "r", "repeat": { "count": 20, "var": "N", "pitch": [0, 12.98] },
              "visible": { "view_model": "view_model_data.hud_tt_r{NN}_on" },
              "children": [
                { "kind": "text", "id": "pos", "style": "OSD_19_regular", "cap_height": 5.9, "at": [5.3, 9.7],
                  "value": { "view_model": "view_model_data.hud_tt_r{NN}_pos" } } ] } ] },

        { "kind": "group", "id": "up", "at": [30, 24], "visible": { "view_model": "view_model_data.hud_tt_up" },
          "motion": {
            "id": "in",
            "keys": [[0, 0], [0.15, 1]],                      // alpha from 0 to 1; the curve sets the time
            "curve": "generic_fade_on_slow",
            "play_on": { "view_model": "view_model_data.hud_tt_up" }
          },
          "children": [
            { "kind": "sprite", "id": "arrow", "sprite": "arrow_up", "layer": 30 } ] }
      ]
    }
  }
}
```

A key is `[time, value]`. A value that is a point `[x, y]` makes the motion a slide: the group's children move by that offset, in HUD units, in the group's own space. A value that is a number makes it a fade: the children's alpha goes from the first key's value to the last key's. The two kinds never mix in one motion, and a fade has exactly two keys, because the game's alpha override takes a start and an end and lets the curve time them.

The curve is one the stock UI templates name: `generic_fade_on`, `generic_fade_off`, `generic_fade_on_slow`, `close_screen_fade`, `open_screen_fade`, `vc_team_bar_slide_in`, `tree_zoom_in`, `smart_text_fade`, `default_curve_1` and the rest of the `broadcast_*`, `*_screen_fade` and `rwds_xp_*` set (`swofty_core::hud::motion::STOCK_CURVES`). Any other name stops the build: the game knows a curve by the hash of its name and draws nothing for a name it does not have.

## What the composer writes

```text
scene     <prefix>_rows                       the group's null, as without motion: its visible node, its place, its box
            zh_<prefix>_rows_swap             a holder null the motion targets; the group's children live in it
              ...                             for a slide, the holder carries anims -> <scene>/zh_<prefix>_rows_swap?context=animation
clip      <scene>/zh_<prefix>_rows_swap?context=animation      position keys [0, (0, 0)], [0.75, (0, -12.98 * k)]; one rotation and one scale key
          <scene>?context=animstringtable&file=0             the holder's name appended to the scene's anim string table

template  <prefix>_ctl
            m_rows_swap        UIXA  over zh_<prefix>_rows_swap                      the player
            mv_rows_swap       MVCD  dbEntry #view_model_data.hud_tt_swap
                                 event 1 -> m_rows_swap.playToFraction { curveName: djb2("generic_fade_on"), value: 1.0 }
                                 event 0 -> m_rows_swap.playToFraction { curveName: djb2("generic_fade_on"), value: 0.0 }
            m_up_in_on         UIMO  over zh_<prefix>_up_in   Alpha, Multiplicative, Cascade, StartingValue 0, Value 1, AnimationCurve generic_fade_on_slow
            m_up_in_off        UIMO  over zh_<prefix>_up_in   the same from 1 back to 0
            mv_up_in           MVCD  dbEntry #view_model_data.hud_tt_up
                                 event 1 -> m_up_in_on.activateOverride { overrideInstant: false }
                                 event 0 -> m_up_in_off.activateOverride { overrideInstant: false }
            commands trigger / untrigger       register + query / unregister the mv_ nodes, with the record's decision nodes
```

The players and the triggers are shaped on the stock HUD template: its `show_mfd_background` and `hide_mfd_background` nodes (an Alpha override pair started by `activateOverride`), its `show_penalty_group` node (a decision on a bool, with events `1` and `0`) and the `UIXA` players its decisions send `goToFraction` and `playToFraction` to. The holder's name starts with `zh_`, which sorts after every stock name in the scene's anim string table, the way the gauge and sweep nulls do.

On a screen record the players sit under `<prefix>_ctl.host_n`, inside the instance, and the `trigger` and `untrigger` commands of the control group register the triggers when the stock element is enabled.

## Rules

- A motion goes on a `group`, and the group may not be a direct child of a `stack`: the stacker lays its children out itself and would put the holder back. Wrap the moving group in a plain group inside the stack instead.
- Keys are local: the offset is from the group's own place, so a HUD customisation move, resize or opacity change still applies to the whole group, moving or not.
- A fade multiplies the player's opacity setting (`BlendType` Multiplicative) rather than replacing it, and it is applied to the holder null, never to the addon's own materials, so a sprite's texture and tint stay as drawn.
- The editor copy (the 5-row preview in HUD customisation) is built without holders, players or triggers: it is static.
- `play_on` names a `view_model` bool leaf, without `not` or `default`; the leaf going true plays the motion and going false plays it back.
- A scene target (`scene` + `parent_ui`) takes no motion: its nodes have no control group to register the triggers from. Dial records and screen records do.
- Time and value are checked at build time: two keys or more, the first at 0, times going up, alpha within 0 and 1, a known curve.

::: warning Not yet seen in game
A decision node (`MVCD`) on a leaf of the addon's own view model, as the triggers here are, has not been seen working in game yet; the stock template only decides on the game's own values. The players and the clips are the stock shapes. Test a motion in a session before shipping it, and keep the leaf false until the row is placed.
:::

The `hud.element` schema is at `1.1`; `motion` is an optional field on an element, see [HUD elements](/reference/ops/hud) for the rest of the record.
