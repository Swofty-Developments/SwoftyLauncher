---
title: swofty.json
description: The manifest of an Overcooked 2 addon. Its id, name and version, compat.game "overcooked_2", the standard library range, and the assemblies Swofty Core loads in order.
---

# swofty.json

An Overcooked 2 addon is a folder or a zip with `swofty.json` at its root, next to its assemblies.

```text
yourname.party-mode/
├── swofty.json
├── PartyMode.dll
├── lib/
│   └── PartyMode.Recipes.dll
└── icon.png
```

```jsonc [swofty.json]
{
  "id": "yourname.party-mode",                // at least two dot-separated parts
  "name": "Party Mode",
  "version": "1.2.0",
  "author": "Your Name",
  "description": "A party toggle on the main menu.",
  "compat": { "game": "overcooked_2", "std": ">=0.1.0" },
  "assemblies": ["PartyMode.dll", "lib/PartyMode.Recipes.dll"],   // loaded in this order
  "icon": "icon.png",
  "requires": [{ "id": "yourname.recipe-lib", "version": ">=1.0.0" }]
}
```

The comments are for this page; `swofty.json` is plain JSON.

## Fields

- **`id`**: the addon's id, also its folder name under `$DATA/games/overcooked_2/addons/`, its log tag and its Harmony id.
- **`name`**, **`version`**, **`author`**, **`description`**: shown in the Library, the Addon Browser and the **Addons** dialog. `version` is semver.
- **`compat.game`**: `"overcooked_2"`. The launcher refuses an addon without it, and an F1 25 addon (`"f1_25"`) for Overcooked 2.
- **`compat.std`**: the semver range of the standard library the addon builds against.
- **`assemblies`**: paths relative to the addon folder. Swofty Core loads them in this order and fails the addon when one is missing or outside the folder.
- **`icon`**: a png in the addon folder. Optional.
- **`requires`**: other addons that must be installed, as `[{ "id", "version" }]`, the same as [F1 25's `requires`](/reference/addons/manifest#requires-conflicts-load-after). Optional.
- **`kind`**: ignored for Overcooked 2.

## Errors in core.log

```text [core.log]
00:15:07 [core] yourname.party-mode failed: the addon lists no assemblies
00:15:07 [core] yourname.party-mode failed: missing assembly lib/PartyMode.Recipes.dll
00:15:07 [core] yourname.party-mode failed: assembly path ../x.dll is not inside the addon folder
00:15:07 [core] yourname.party-mode failed: no public class implements Swofty.Overcooked2.IAddon
```
