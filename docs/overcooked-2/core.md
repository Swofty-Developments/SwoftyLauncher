---
title: How the core loads
description: Play starts Overcooked 2 with Unity Doorstop, which runs Swofty Core before the game. Swofty Core reads the session file, loads the standard library and each addon, patches the game with Harmony and writes core.log and status.json.
---

# How the core loads

**Play** starts Overcooked 2 with [Unity Doorstop](https://github.com/NeighTools/UnityDoorstop) preloaded. Doorstop runs `Swofty.Overcooked2.Core.dll` inside the game's Mono runtime before the game's own code. Swofty Core then reads the session file the launcher wrote, loads `Swofty.Overcooked2.dll` and every addon that is on, and patches the game with [Harmony](https://github.com/pardeike/Harmony).

```text
launcher ── writes ──▶ sessions/<stamp>/session.json
    │
    └─ starts Overcooked2.x86_64 with LD_PRELOAD=libdoorstop.so
           │
           └─ Doorstop ─▶ Swofty.Overcooked2.Core.dll   Doorstop.Entrypoint.Start
                              ├─ reads session.json
                              ├─ loads Swofty.Overcooked2.dll and 0Harmony.dll
                              ├─ loads each addon's assemblies, calls Load on each IAddon
                              ├─ patches the main menu: the Addons button and every MainMenu.AddButton
                              └─ writes core.log and status.json
```

## The files

The launcher ships Swofty Core in `cores/overcooked_2/`, next to its `bin/` folder:

```text
cores/overcooked_2/
├── core.json                          {"core": "0.1.0", "std": "0.1.0", "harmony": "…", "doorstop": "…"}
├── core/
│   ├── Swofty.Overcooked2.Core.dll    the Doorstop target
│   ├── Swofty.Overcooked2.dll         the standard library
│   └── 0Harmony.dll
└── doorstop/
    ├── linux_x64/libdoorstop.so
    └── windows_x64/winhttp.dll
```

`SWOFTY_OC2_CORE_DIR` points the launcher at another copy of that folder, such as `games/overcooked_2/dist/` in a source checkout after `games/overcooked_2/build.sh`.

## The launch

On the native Linux build, the launcher starts `Overcooked2.x86_64` from the game folder with:

```sh
LD_PRELOAD=$CORE/doorstop/linux_x64/libdoorstop.so
DOORSTOP_ENABLED=1
DOORSTOP_TARGET_ASSEMBLY=$CORE/core/Swofty.Overcooked2.Core.dll
SteamAppId=728880
SteamGameId=728880
SWOFTY_OC2_SESSION=$DATA/games/overcooked_2/sessions/<stamp>/session.json
```

Steam must be running. Nothing in the game folder changes.

On the Windows build, the launcher copies `winhttp.dll` into the game folder, writes `doorstop_config.ini` next to `Overcooked2.exe`, starts the game, and removes both files when it exits:

```ini [doorstop_config.ini]
[General]
enabled = true
target_assembly = C:\…\cores\overcooked_2\core\Swofty.Overcooked2.Core.dll
```

## The session file

`SWOFTY_OC2_SESSION` names the session file. The addons are the ones that are on, in load order:

```json [session.json]
{
  "schema": 1,
  "launcher": "1.10.63",
  "game_dir": "/home/you/.local/share/Steam/steamapps/common/Overcooked! 2",
  "data_dir": "/home/you/.local/share/swofty/games/overcooked_2",
  "log": "/home/you/.local/share/swofty/games/overcooked_2/sessions/20261129-001500/core.log",
  "status": "/home/you/.local/share/swofty/games/overcooked_2/sessions/20261129-001500/status.json",
  "addons": [
    {
      "id": "swofty.hello-kitchen",
      "name": "Hello Kitchen",
      "version": "0.1.0",
      "dir": "/home/you/.local/share/swofty/games/overcooked_2/addons/swofty.hello-kitchen",
      "assemblies": ["HelloKitchen.dll"]
    }
  ]
}
```

For each addon, Swofty Core loads its `assemblies` in order, creates every public non-abstract class that implements [`IAddon`](/overcooked-2/std/), and calls `Load`. An exception in an addon fails that addon only; the rest load.

Without a session file, or with one it cannot read, Swofty Core writes to Unity's `Player.log`, loads no addons, and still adds its **Addons** button.

## Logs

Each session has its own folder, `$DATA/games/overcooked_2/sessions/<stamp>/`. Swofty Core appends to `core.log` in it, one line per message, tagged `core` or with the addon id:

```text [core.log]
00:15:07 [core] Swofty Core 0.1.0, std 0.1.0, Harmony 2.3.6, launcher 1.10.63
00:15:07 [core] Overcooked 2 build 20236421
00:15:07 [core] loading swofty.hello-kitchen 0.1.0
00:15:07 [swofty.hello-kitchen] loaded on Overcooked 2 build 20236421
00:15:07 [core] loaded 1 addon, 0 failed
00:15:21 [swofty.hello-kitchen] scene StartScreen
00:15:29 [swofty.hello-kitchen] scene frontend
00:15:29 [core] main menu shown with 2 added buttons
```

`Log.Warn` lines start with `warning:` and `Log.Error` lines with `error:`.

Swofty Core writes `status.json` once the addons have loaded, and again when the main menu first shows:

```json [status.json]
{
  "core": "0.1.0",
  "std": "0.1.0",
  "loaded": ["swofty.hello-kitchen"],
  "failed": [],
  "main_menu": true
}
```

A failed addon is in `failed` with its error:

```json
{ "id": "yourname.broken", "error": "NullReferenceException: Object reference not set to an instance of an object" }
```
