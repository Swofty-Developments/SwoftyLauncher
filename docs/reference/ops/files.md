---
title: File operations
description: Replace or add loose game files from an addon with operations.files or file.put, and the build lines they print.
---

# File operations

File operations copy loose files from an addon into the game folder.

## File operations in full

```jsonc [swofty.json]
"operations": {
  "files": [
    { "source": "files/ferrari_2024.bdl",          // a file in the addon, relative to the addon folder, / separators
      "target": "audio/2025/bundles/ferrari_2024.bdl" } // where it goes, relative to the game folder
  ]
}
```

The same change as an op in an `assets` file, as Classic Seasons ships its car archives:

```json [assets/cars.json]
{
  "schema_version": 1,
  "ops": [
    { "op": "file.put", "source": "cars/teams/common.erp",
      "target": "2025_asset_groups/cs_2018_vehicle_package/teams/common.erp" }
  ]
}
```

`operations.files` is one kind of [operation](/reference/data/operations): each entry becomes a [`file.put`](/reference/addons/asset-api#op-files) asset op. A `file.put` source can also be an object with a `root` ([Sources](/reference/data/operations#sources)).

## What the build does

```text
files: 1 game file(s) replaced, 0 added
overlay 9c41e0b2…: $DATA/cache/overlay/modded/9c41e0b2…
  replace audio/2025/bundles/ferrari_2024.bdl
```

- The build copies the file into the game folder through the journal, and `swofty restore` removes it or puts the stock file back.
- With the experimental runtime `file` feature turned on (`"runtime_assets": ["file"]` in `$DATA/config.json`), swofty_core serves the file from `swofty/overlay/` instead, and the game folder keeps its stock files.
- The last write to a target wins, and the build lists the conflict.
- A `source` outside the addon folder fails the manifest check.
- The build leaves out a `file.put` to `game.dat` or `game.daf`, the files that hold F1 25's packed game data:

```text
author.mod: assets/files.json: refused file.put game.dat: F1 25 can't start with its game.dat replaced
```

- An addon that puts `game.dat` or `game.daf` and whose files are made for another F1 game, such as files under `2020_asset_groups/`, doesn't install. An addon already installed has all its file operations left out of the build:

```text
Pastoot is made for F1 2020, not F1 25 (it has 2020_asset_groups), and it replaces game.dat, F1 25's packed game data, which F1 25 can't start with replaced; the build left out all 12567 of its files. Turn Pastoot off in the profile
```

- A target that a track, the database or an archive edit already replaces fails the build:

```text
audio/2025/bundles/ferrari_2024.bdl is replaced twice
```
