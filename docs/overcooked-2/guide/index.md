---
title: Introduction
description: What Swofty Launcher does for Overcooked 2 today. The native Linux build plays with Swofty Core, which loads C# addons, patches the game with Harmony and adds a MODS tab to the main menu.
---

# Swofty Launcher for Overcooked 2

Swofty Launcher runs [Overcooked 2](https://store.steampowered.com/app/728880/) with **Swofty Core**, a small in-game loader. Swofty Core loads your addons, patches the game with [Harmony](https://github.com/pardeike/Harmony), and adds a **MODS** tab to the main menu. Its **Addons** button lists every addon and its state.

An addon is a .NET 3.5 assembly with a `swofty.json`. It builds against `Swofty.Overcooked2.dll`, the standard library.

```csharp
using Swofty.Overcooked2;

public sealed class HelloKitchen : IAddon
{
    public void Load(AddonContext ctx)
    {
        MainMenu.AddButton("swofty.hello-kitchen.hello", "Hello", () =>
            Dialogs.Message("Hello Kitchen", "Hello from " + ctx.Name + " " + ctx.Version));
    }
}
```

## What works today

- **The native Linux build** from Steam (`Overcooked2.x86_64`). The launcher finds it, Play starts it with Swofty Core, and Stop closes it.
- **The Library**: install addons from a zip or a folder, turn them on and off, uninstall them.
- **The Addon Browser** for Overcooked 2. It lists addons once the registry opens to the game.
- **Swofty Core** 0.1.0: the **MODS** tab with the **Addons** dialog, `core.log` and `status.json` for every session.
- **The standard library** 0.1.0: [buttons on the MODS tab](/overcooked-2/std/main-menu), [dialogs](/overcooked-2/std/dialogs), [scene events](/overcooked-2/std/game), [versions and the addon list](/overcooked-2/std/runtime), a [log and a data folder per addon](/overcooked-2/std/addon-context), and [Harmony](/overcooked-2/std/harmony).

The Windows build (`Overcooked2.exe`) has a launch path that is not tested yet. The Windows build under Proton on Linux is not supported yet.

F1 25's addons, profiles and data stay as they are. Overcooked 2 keeps its own under `$DATA/games/overcooked_2/`.

## Get started

1. [Install the launcher](/guide/installation).
2. [Pick Overcooked 2](/overcooked-2/guide/playing) in the bottom-left of the launcher, add addons in the Library and press **Play**.
3. To make an addon, start with [Your first addon](/overcooked-2/addons/).
