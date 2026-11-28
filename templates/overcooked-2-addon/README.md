# Overcooked 2 addon

A starting point for an Overcooked 2 addon for Swofty Launcher. Copy this directory, rename it, and change `id` in `swofty.json` and the assembly name in `MyKitchen.csproj` and `swofty.json`.

## Contents

| Path | What it is |
|---|---|
| `swofty.json` | The manifest: id, version, `compat.game` `overcooked_2` with the std lib version it needs, and the assemblies to load |
| `MyKitchen.csproj` | A `net35` class library against the std lib `Swofty.Overcooked2.dll` and Harmony from Swofty Core |
| `src/MyKitchenAddon.cs` | The addon: a **My Kitchen** button on the main menu's MODS tab that opens a dialog, scene and menu events, a counter in its data folder and a Harmony postfix |

## Build and install

```sh
dotnet build -c Release
```

The build needs Swofty Core's `dist/` for the std lib and Harmony. Inside a SwoftyLauncher checkout it uses `games/overcooked_2/dist` (run `games/overcooked_2/build.sh` once). Elsewhere, point it at the core the launcher ships:

```sh
dotnet build -c Release -p:SwoftyCoreDir=/path/to/SwoftyLauncher/cores/overcooked_2
```

The build writes the addon folder to `dist/`: `swofty.json` and `MyKitchen.dll`. Install that folder, or a zip of it, from the launcher's Overcooked 2 library, or with the CLI:

```sh
swofty --game overcooked_2 addons install dist
swofty --game overcooked_2 addons enable example.my-kitchen
```

The addon's log lines go to the session's `core.log` under its id, and to the game's `Player.log`.

## Game types

The template compiles without the game. Reach game classes by name with Harmony's `AccessTools`, as `src/MyKitchenAddon.cs` does. To use them directly, point `OC2_DIR` at the game folder; when `Overcooked2_Data/Managed/Assembly-CSharp.dll` is there the project references it and the game's `UnityEngine*.dll`:

```sh
dotnet build -c Release -p:OC2_DIR="$HOME/.local/share/Steam/steamapps/common/Overcooked! 2"
```

The default is the Steam library folder of each platform. Never ship the game's assemblies with an addon.

## Rules

* Load everything in `Load`. An exception there fails only this addon: Swofty Core removes its menu buttons, its event handlers and its Harmony patches, and lists it as failed in the **Addons** dialog.
* One `IAddon` class per assembly is usual. Every public non-abstract class that implements it is created and loaded.
* Write files only under `ctx.DataDirectory`. The addon folder can be replaced on update.
* Button ids are global. Prefix them with the addon id.

The std lib API is documented under Modding in the Swofty Launcher documentation.

## Licence

This template is under the MIT licence, see [LICENSE](LICENSE), so addons built from it can use any licence.
