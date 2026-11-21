---
title: HUD slots
description: Give an addon its own entry in the game's HUD editor with a hud.slot record, and read the placement the player gives it with swofty/hud_slot.hpp.
---

# HUD slots

A `hud.slot` record adds an entry of your own to the game's HUD editor, the screen where the player moves and resizes the dial, the minimap and the other widgets. The entry has its own selection box. The player moves it, resizes it and switches it off like a stock widget, and the game keeps that placement with its own. Your native code reads the placement with `swofty/hud_slot.hpp` and draws there with the [overlay](./overlay).

```json [swofty.json]
"operations": {
  "hud": ["hud/slots.json"],
  "native": [{ "dll": "native/radio.dll" }]
},
"experimental_features": ["overlay"]
```

```jsonc [hud/slots.json]
{
  "hud.slot": {
    "radio": {
      "name": "Team radio",                 // the editor's row
      "rect": [0.02, 0.55, 0.28, 0.12],     // x, y, width, height as fractions of a 1920x1080 frame
      "anchor": "left"                      // what the box keeps its distance to when the frame changes shape
    }
  }
}
```

```cpp [native/src/radio.cpp]
#include <swofty/hud_slot.hpp>
#include <swofty/mixin.hpp>
#include <swofty/overlay.hpp>

namespace hs = swofty::hud_slot;
namespace ov = swofty::overlay;

// The same id, rect and anchor as the record
constexpr hs::Slot kRadio{"radio", "Team radio", 0.02f, 0.55f, 0.28f, 0.12f, hs::Anchor::left};

bool named = false;

void paint(SwoftyPaint *p, void *)
{
    if (!named)
        named = hs::declare(kRadio) == hs::Result::ok;   // the editor row reads "Team radio"
    float w, h;
    ov::size(p, &w, &h);
    hs::Placement at;
    hs::get(kRadio, w, h, at);                            // framebuffer pixels
    if (!at.visible)
        return;                                           // the player switched it off
    if (at.editor_open) {                                 // a preview while the player edits
        ov::rect(p, at.x, at.y, at.w, at.h, 0xff800090);
        ov::text(p, at.x + 10, at.y + 8, 24 * h / 1080, 0xffffffff, "TEAM RADIO");
        return;
    }
    draw_card(p, at);                                     // your own drawing, at.x, at.y, at.w, at.h
}

SWOFTY_ON_LOAD
{
    ov::on_paint(paint);
}

SWOFTY_ADDON()
```

## The record

| Key | Default | Meaning |
|---|---|---|
| `name` | required | the text of the editor's row; `declare` sets it while the game runs |
| `rect` | required | `[x, y, w, h]`: the default box as fractions of the frame from its top-left, inside the frame |
| `anchor` | `centre` | `top_left`, `top`, `top_right`, `left`, `centre`, `right`, `bottom_left`, `bottom` or `bottom_right` |

The entry draws nothing of its own: in the editor the player sees its selection box and whatever your plugin draws while `editor_open` is true. A slot's id is the key under `hud.slot`. A `hud.slot` map sits in any file `operations.hud` lists, beside or instead of `hud.element` records, and the file's [`when`](/reference/ops/hud#a-hud-file-behind-a-setting) holds for both.

## What the build adds

For each slot the build writes three changes into the game's UI archives:

| Resource | Archives | Change |
|---|---|---|
| `art/osd/components/swofty/swofty_<addon>_<id>.idf` | `flow_race.erp`, `flow_options.erp`, `persistent.erp` | a new scene with only `boundless_customisation`, `tl_customisation` and `br_customisation`; the two points are the corners of the default box |
| `templates/hud_dynamic_templates.ui` | `flow_race.erp`, `flow_options.erp` | a template group `swofty_<addon>_<id>` that instances the scene |
| `art/osd/screens/hud_layouts/hud_layouts.uidl` | `persistent.erp` | a component in the `default` layout: the anchor, the template, `dynamicKey` and `nameId` `swofty_<addon>_<id>`, and the position of the box centre |

`<addon>` and `<id>` are lowercased, with every character other than a letter or digit turned into `_`. The build log lists each slot with its keys:

```text
hud slot: swofty_acme_radio_radio of acme.radio: "Team radio" at [0.02, 0.55, 0.28, 0.12] from anchor_left, keys race 141999741, qualifying 4011009979, practice 2953617677, time_trial 1422233004, elimination 899772539
```

Three archives change their stock layout, so the build writes them into the game files (see [how changes reach the game](/reference/ops/erp#how-changes-reach-the-game)). The HUD composer's `hud.element` records compose on top of the same template.

## Where the placement lives

The game keeps HUD placement in `Documents/My Games/F1 25/hud_customisation/hud_customisation.xml` in the game's prefix, one `HudElement` per widget and session layout. A slot has one entry per session layout (`race`, `qualifying`, `practice`, `time_trial`, `elimination`). The key is `djb2(layout + "swofty_<addon>_<id>")`, the hash the game uses for its own widgets: `djb2("race" + "dial_f1")` is `647083312`. `x` and `y` are the box centre's offset from its anchor in HUD units, `y` up; the HUD frame is 19.2 by 10.8 units. `scale` (0.5 to 2) and `enabled` are the player's.

The file is not the profile save, and Steam Cloud does not sync it. After the addon is off, the game loads its entries and drops them at its next save.

## Calls

| Call | Does |
|---|---|
| `available()` | `true` when the core exports `SwoftyCoreHudSlotRead` |
| `get(slot, fb_w, fb_h, placement)` | fills `placement` for a frame of `fb_w` by `fb_h` pixels; `Result::ok` when the game has the slot, `no_slot` before the game loaded its HUD placement (`placement` is the default box then), `unavailable` without the export |
| `read(slot, state)` | the raw `SwoftyHudSlotState` the core reads |
| `place(slot, state, fb_w, fb_h)` | turns a state into a `Placement`; no core needed, for tests |
| `editor_open()` | `true` while the HUD editor is open |
| `declare(slot)` | sets the editor row's text, `lng_hud_component_swofty_<addon>_<id>`, to `slot.name` with [`swofty::lang`](./strings) |

`Placement`:

| Field | Meaning |
|---|---|
| `x`, `y`, `w`, `h` | the box in framebuffer pixels from the top-left |
| `scale`, `opacity` | the player's scale and opacity |
| `visible` | the player has the slot on |
| `editor_open` | the HUD editor is open |
| `live` | the values come from the game, not from the slot's defaults |

While the editor is open the box is the editor's own selection box, which follows the player's drag and resize as they happen. Elsewhere it is the slot's default size times `scale`, centred on the anchor plus `x` and `y`.

`SwoftyHudSlotState` holds what the core reads: `found`, `enabled`, `editor_open`, `x`, `y`, `scale`, `opacity`, `has_box` with `box` (min x, min y, max x, max y in HUD units, origin at the frame centre, `y` up), `element_key` (the `hud_customisation.xml` key in use) and `key` (`swofty_<addon>_<id>`). The core picks the session layout whose element list has the slot, from the editor's layout while it is open.

The core calls into the game only on the flow thread (the one that runs `on_frame`), so `read` and `get` are guarded memory reads on any other thread, the paint callback's included. Until the first flow tick after the first read, `get` gives the default box.
