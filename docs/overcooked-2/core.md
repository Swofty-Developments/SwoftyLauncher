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
                              ├─ adds the MODS tab to the main menu: Addons and every MainMenu.AddButton
                              └─ writes core.log and status.json
```

## The files

The launcher ships Swofty Core in `cores/overcooked_2/` in its [install folder](/guide/installation#the-install-folder), next to `bin/`:

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

Swofty Core loads the addons when the game loads its first scene, `Boot`, on Unity's main thread. For each addon in order, it loads its `assemblies` in order, creates every public non-abstract class that implements [`IAddon`](/overcooked-2/std/), and calls `Load`. An exception in an addon fails that addon only; the rest load.

Without a session file, or with one it cannot read, Swofty Core writes to Unity's `Player.log`, loads no addons, and still adds the **MODS** tab with **Addons**.

## Logs

Each session has its own folder, `$DATA/games/overcooked_2/sessions/<stamp>/`. Swofty Core appends to `core.log` in it, one line per message, tagged `core` or with the addon id:

```text [core.log]
00:15:01 [core] Swofty Core 0.1.0 for Overcooked 2 (std 0.1.0, Harmony 2.3.6, Doorstop 4.5.0)
00:15:01 [core] session /home/you/.local/share/swofty/games/overcooked_2/sessions/20261129-001500/session.json from launcher 1.10.63, 1 addon(s)
00:15:02 [core] Overcooked 2 build 66.678012
00:15:03 [core] first scene Boot; loading addons
00:15:03 [swofty.hello-kitchen] Hello Kitchen 0.1.0 loaded on Overcooked 2 build 66.678012 with Swofty Core 0.1.0
00:15:03 [core] loaded swofty.hello-kitchen 0.1.0
00:15:03 [core] addons: 1 loaded, 0 failed
00:15:03 [swofty.hello-kitchen] scene loaded: Boot
00:15:06 [core] scene Ident loaded (Single)
00:15:06 [swofty.hello-kitchen] scene loaded: Ident
00:15:19 [core] scene StartScreen loaded (Single)
00:15:19 [swofty.hello-kitchen] scene loaded: StartScreen
00:15:20 [core] scene StartScreen_Base loaded (Additive)
00:15:20 [swofty.hello-kitchen] scene loaded: StartScreen_Base
00:15:25 [core] added the MODS tab to the main menu with 2 button(s)
00:15:25 [swofty.hello-kitchen] the front end is up
00:15:25 [core] main menu shown
00:15:25 [swofty.hello-kitchen] the main menu has shown
00:15:50 [core] main menu button swofty.hello-kitchen.hello clicked
00:15:50 [swofty.hello-kitchen] hello number 1
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
