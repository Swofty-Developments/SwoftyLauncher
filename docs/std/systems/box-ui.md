---
title: Box UI textures
description: A Grand Prix or Time Trial box's own UI textures (FIA and F1 logos, broadcast graphics, loading screen and menu art), as a ui.per_package record that swaps them into ui_package archives while the box is pressed.
---

# Box UI textures

Give one box its own UI art: a 1950 season box shows a period FIA logo on the race control messages, the broadcast graphics and the safety car banner, while the stock 2025 box and every other box keep the stock art. Experimental: off unless the game starts with `SWOFTY_BOX_UI=1`.

A few terms used on this page:

- A **box** is one of the year tiles in Grand Prix or Time Trial. Its season build is a `content.package` record.
- A **UI archive** is one of the game's `2025_asset_groups/ui_package/**.erp` files. The same texture often sits in several of them: `bc_logos_fia.tif.image` is in `flow_race.erp` and `screens/broadcast_persistent.erp`.
- A **copy** is the core's version of a UI archive with the box's textures swapped in. Every other resource keeps its stock bytes.

## The header

<Example root="swofty-sdk" code-file="include/swofty/systems/content.hpp">
<template #code>

```cpp
#include <swofty/registry.hpp>

namespace swofty::content {

struct PackageUi {                                 // one box's own UI textures
    Ref<Package> package;                          // the box's content package
    Path textures;                                 // a folder of .dds / .png files, or { erp, resources }
    std::string root = "eaid://ui_package/art/";   // a folder file's path is appended to this
    bool srgb = true;                              // a .png is encoded as sRGB BC7
    std::vector<std::string> archives;             // the ui_package archives to swap
};

inline Registry<PackageUi> package_ui{"ui.per_package", { .applies = Applies::content }};

}
```

</template>
</Example>

A folder file's path below the folder, without its extension, is appended to `root`: `osd/textures/message_system/ms_fia_logo.png` replaces `eaid://ui_package/art/osd/textures/message_system/ms_fia_logo.tif.image` and its view `ms_fia_logo.tif`. A surface and its view are swapped together; a resource must keep its stock type and fragment count (UI sprites: a 40 byte header and the pixels, no streaming descriptor). The other form, `{ "erp": "ui/f5_1950_ui.erp", "resources": [...] }`, takes the named resources of an archive the addon ships.

## The 1950 box, as data

```json
// registry/ui.json
{
  "ui.per_package": {
    "f5_1950_ui": {
      "package": "vmknolle.season-1950/f5_1950",
      "textures": "ui/f5_1950",
      "archives": [
        "2025_asset_groups/ui_package/flow_race.erp",
        "2025_asset_groups/ui_package/screens/broadcast_persistent.erp",
        "2025_asset_groups/ui_package/flow_race_intro.erp",
        "2025_asset_groups/ui_package/screens/broadcast_intros.erp"
      ]
    }
  }
}
```

with `ui/f5_1950/textures/broadcast_ui_generic/bc_logos/bc_logos_fia.png`, `.../bc_safety_car/bc_sc_fia_icon.png` and `.../bc_generic_medium/bc_small_f1_logo.png` in the addon folder.

## What the core does

- **At the box press** (the `Applies::content` activation of the box) it reads the box's records: those whose `package` is the pressed package, or the first package along its variant chain that has any.
- **For every listed archive** it opens the file the game would open now (another route's copy, such as a `team.logo` copy, else the game folder's), swaps every resource of the textures that the archive also holds, and writes the copy to `swofty/addons/swofty.box_ui/boxui_<key>_<archive>.erp`. The key hashes the record, the textures and the source file's size and time, so an unchanged press reuses the copy.
- **It routes the archive to the copy** through the core's file route in the scope `content:ui.per_package`. The next box press, leaving the box and a career load put the scope back, so the game opens its own archive again.
- **It refuses resident archives** and says which textures stay stock because one of them also holds them.

```text
[apply] ui.per_package activate:   ui.per_package vmknolle.season-1950/f5_1950_ui: 2025_asset_groups/ui_package/flow_race.erp -> swofty/addons/swofty.box_ui/boxui_3f0c..._flow_race.erp (4 resource(s), written in 61 ms)
[apply] ui.per_package activate:   ui.per_package vmknolle.season-1950/f5_1950_ui: eaid://ui_package/art/osd/textures/message_system/ms_fia_logo.tif.image stays stock: 2025_asset_groups/ui_package/persistent.erp also holds it, and the game keeps the copy it loads at boot
[apply] ui.per_package deactivate:   ui.per_package: put back 2025_asset_groups/ui_package/flow_race.erp: the game's own file again
```

## When the game reads a copy

A route changes what the next open of the archive reads. The game's FLOW log (`FlowCoordinator::processNextPreload`) shows when it opens the UI flows:

| state | when | what it does |
|---|---|---|
| 1 | `StatePersistent`, once at boot | loads one flow; no later state unloads it |
| 2 | the front end | loads the front-end flows (again after every session) |
| 4 | each session load from the front end | unloads a front-end flow, loads the race flows and the `hud` screen elements; they unload when the session goes back to the front end. A session after a session reloads nothing. |

So:

- **Race UI** (`flow_race.erp`, `flow_race_intro.erp`, `screens/hud*.erp`, broadcast screens) takes the box's art at the box's first session and keeps it for the weekend.
- **Menus** (`flow_frontend.erp`, `screens/fe_*.erp`, `screens/zenith_*.erp`) are already loaded when the box is pressed: their copy shows after the next session, when the front end loads again, and the stock art comes back the same way after another box is pressed.
- **Loading screens** (`flow_loading.erp`, `screens/loading*.erp`): whether they reload for every load is not seen yet (T357).

## What it cannot swap

- **Resident archives:** `persistent.erp`, `flow_persistent.erp`, `screens/persistent.erp` and the `fonts*.erp` archives load at boot (preload state 1 and `UIFontAssetsSubTask` in `StateEgoBoot`) and are never opened again. A record that lists one is refused for that archive.
- **Textures a resident archive also holds:** the resource map keeps the first resource created for a name (re/db/subsystems/assets.md), so a copy's texture of the same name is never used. Of the F1 / FIA logos: `ms_fia_logo` and `lc_f1_logo` (also in `persistent.erp`), `bc_medium_f1_logo` (only in `persistent.erp`) and `zenith/f1_logo_cta` (also in `flow_persistent.erp` and `screens/persistent.erp`) stay stock. A route for these needs a different mechanism (branding `texture_override` rows or an own HUD element), not a file swap.
- **Archives the record does not list:** a texture that another, unlisted archive loads first keeps that archive's stock copy. List every archive that holds it.
- **Careers:** a career's activation swaps nothing in this release.

## What is known

- **Off by default:** without `SWOFTY_BOX_UI=1` a box with records logs one line and swaps nothing.
- **Built and tested offline:** record parsing, the resident list, the swap plan (surface and view together, same type and fragment count) and the rewritten archive run in the host test `box_ui_test`.
- **Not seen in game yet:** that the race UI shows the copy's textures (T355), that leaving the box shows stock art again at the next session (T356), loading screens (T357), menus after one session (T358), and no crash or missing texture with a copy whose texture has another size than the stock one (T359).
