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

```json [swofty.json]
{
  "id": "swofty.hello-kitchen",
  "name": "Hello Kitchen",
  "version": "0.1.0",
  "author": "Swofty",
  "description": "Adds a Hello button to the main menu.",
  "compat": { "game": "overcooked_2", "std": ">=0.1.0" },
  "assemblies": ["HelloKitchen.dll"]
}
```

```xml [HelloKitchen.csproj]
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net35</TargetFramework>
    <AssemblyName>HelloKitchen</AssemblyName>
    <RootNamespace>HelloKitchen</RootNamespace>
    <Version>0.1.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net35" Version="1.0.3" PrivateAssets="all" />
  </ItemGroup>
  <ItemGroup>
    <ProjectReference Include="../../std/Swofty.Overcooked2.csproj" Private="false" />
  </ItemGroup>
</Project>
```

```csharp [HelloKitchen.cs]
using System;
using System.Globalization;
using System.IO;
using HarmonyLib;
using Swofty.Overcooked2;

namespace HelloKitchen
{
    public sealed class HelloKitchenAddon : IAddon
    {
        private static AddonContext context;

        public void Load(AddonContext ctx)
        {
            context = ctx;
            ctx.Log.Info(ctx.Name + " " + ctx.Version + " loaded on Overcooked 2 build " + Game.Version + " with Swofty Core " + Runtime.CoreVersion);

            MainMenu.AddButton("swofty.hello-kitchen.hello", "Hello", SayHello);
            MainMenu.Shown += () => ctx.Log.Info("the main menu has shown");
            Game.SceneLoaded += scene => ctx.Log.Info("scene loaded: " + scene);

            Type frontend = AccessTools.TypeByName("T17FrontendFlow");
            if (frontend != null)
            {
                ctx.Harmony.Patch(AccessTools.Method(frontend, "Awake"), postfix: new HarmonyMethod(typeof(HelloKitchenAddon), nameof(FrontendAwake)));
            }
        }

        private static void FrontendAwake()
        {
            context.Log.Info("the front end is up");
        }

        private static void SayHello()
        {
            int count = CountHello();
            context.Log.Info("hello number " + count);
            Dialogs.Message(
                "Hello Kitchen",
                "Hello from " + context.Name + " " + context.Version + "!\n"
                + "You have said hello " + count + (count == 1 ? " time" : " times") + ".\n"
                + "Swofty Core " + Runtime.CoreVersion + ", launcher " + (Runtime.LauncherVersion ?? "none") + ".");
        }

        private static int CountHello()
        {
            string file = Path.Combine(context.DataDirectory, "hello-count.txt");
            int count = 0;
            if (File.Exists(file))
            {
                int.TryParse(File.ReadAllText(file).Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out count);
            }
            count++;
            File.WriteAllText(file, count.ToString(CultureInfo.InvariantCulture));
            return count;
        }
    }
}
```

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
- **`HelloKitchen.csproj`** builds `HelloKitchen.dll` for .NET Framework 3.5 against the standard library. Outside the repository, reference the built `Swofty.Overcooked2.dll` instead: see [Building against the std lib](./building).
- **`HelloKitchenAddon`** implements [`IAddon`](/overcooked-2/std/). Swofty Core creates it and calls `Load` when the game loads its first scene.
- **`MainMenu.AddButton`** adds **Hello** to the **MODS** tab, after **Addons**: see [MainMenu](/overcooked-2/std/main-menu).
- **`MainMenu.Shown`** and **`Game.SceneLoaded`** log to `core.log` through `ctx.Log`: see [Game](/overcooked-2/std/game) and [AddonContext and Log](/overcooked-2/std/addon-context).
- **`ctx.Harmony.Patch`** adds a postfix to the game's `T17FrontendFlow.Awake`, found by name: see [Harmony patches](/overcooked-2/std/harmony).
- **`Dialogs.Message`** opens the dialog: see [Dialogs](/overcooked-2/std/dialogs).
- **`ctx.DataDirectory`** keeps `hello-count.txt` between sessions, in `$DATA/games/overcooked_2/addon-data/swofty.hello-kitchen/`.

## Publishing

The Addon Browser for Overcooked 2 takes addons once the registry opens to the game.
