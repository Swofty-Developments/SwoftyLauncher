---
title: Your first addon
description: Build Hello Kitchen, the example Overcooked 2 addon. It adds a Hello button to the MODS tab of the main menu that opens a dialog, logs every scene, keeps a count in its data folder and patches the game with Harmony. Every file and command first, then what each part does.
---

# Your first addon

Hello Kitchen adds a **Hello** button to the **MODS** tab of the main menu. The button opens a dialog and counts how often you said hello; the addon also logs every scene and patches the front end with Harmony. It is `games/overcooked_2/examples/hello-kitchen` in the repository.

You need the .NET 8 SDK (`dotnet`). `nix develop` in the repository has it.

## The files

```text
hello-kitchen/
├── swofty.json
├── HelloKitchen.csproj
└── HelloKitchen.cs
```

<<< @/../games/overcooked_2/examples/hello-kitchen/swofty.json{json} [swofty.json]

<<< @/../games/overcooked_2/examples/hello-kitchen/HelloKitchen.csproj{xml} [HelloKitchen.csproj]

<<< @/../games/overcooked_2/examples/hello-kitchen/HelloKitchen.cs{csharp} [HelloKitchen.cs]

## Build, install and play

```sh
games/overcooked_2/build.sh                    # Swofty Core into dist/, Hello Kitchen into examples/hello-kitchen/dist/
swofty --game overcooked_2 addons install games/overcooked_2/examples/hello-kitchen/dist
swofty --game overcooked_2 play --wait
```

`examples/hello-kitchen/dist/` holds `swofty.json` and `HelloKitchen.dll`: the addon as the launcher installs it. Zip that folder to share it.

`play --wait` starts the game with Swofty Core and prints `status.json` when it closes; **Play** in the launcher, with Overcooked 2 picked, starts it the same way. The **MODS** tab on the main menu has **ADDONS** and **HELLO**. **HELLO** opens:

```text
Hello Kitchen

Hello from Hello Kitchen 0.1.0!
You have said hello 1 time.
Swofty Core 0.1.0, launcher 1.10.63.
```

```text [core.log]
00:15:03 [core] first scene Boot; loading addons
00:15:03 [swofty.hello-kitchen] Hello Kitchen 0.1.0 loaded on Overcooked 2 build 66.678012 with Swofty Core 0.1.0
00:15:03 [core] loaded swofty.hello-kitchen 0.1.0
00:15:03 [core] addons: 1 loaded, 0 failed
00:15:03 [swofty.hello-kitchen] scene loaded: Boot
00:15:06 [swofty.hello-kitchen] scene loaded: Ident
00:15:19 [swofty.hello-kitchen] scene loaded: StartScreen
00:15:20 [swofty.hello-kitchen] scene loaded: StartScreen_Base
00:15:25 [swofty.hello-kitchen] the front end is up
00:15:25 [swofty.hello-kitchen] the main menu has shown
00:15:50 [core] main menu button swofty.hello-kitchen.hello clicked
00:15:50 [swofty.hello-kitchen] hello number 1
```

## What each part does

- **`swofty.json`** names the addon and its assembly. `compat.game` is `"overcooked_2"`: see [swofty.json](./manifest).
- **`HelloKitchen.csproj`** builds `HelloKitchen.dll` for .NET Framework 3.5 against the standard library. Outside the repository, start from [the template](./template): see [Building against the std lib](./building).
- **`HelloKitchenAddon`** implements [`IAddon`](/overcooked-2/std/). Swofty Core creates it and calls `Load` when the game loads its first scene.
- **`MainMenu.AddButton`** adds **Hello** to the **MODS** tab, after **Addons**: see [MainMenu](/overcooked-2/std/main-menu).
- **`MainMenu.Shown`** and **`Game.SceneLoaded`** log to `core.log` through `ctx.Log`: see [Game](/overcooked-2/std/game) and [AddonContext and Log](/overcooked-2/std/addon-context).
- **`ctx.Harmony.Patch`** adds a postfix to the game's `T17FrontendFlow.Awake`, found by name: see [Harmony patches](/overcooked-2/std/harmony).
- **`Dialogs.Message`** opens the dialog: see [Dialogs](/overcooked-2/std/dialogs).
- **`ctx.DataDirectory`** keeps `hello-count.txt` between sessions, in `$DATA/games/overcooked_2/addon-data/swofty.hello-kitchen/`.

## Publishing

The Addon Browser for Overcooked 2 takes addons once the registry opens to the game.
