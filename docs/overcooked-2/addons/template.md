---
title: The template
description: templates/overcooked-2-addon, a working Overcooked 2 addon to start from. A button on the MODS tab, a dialog, scene and menu events, a counter in its data folder and a Harmony postfix. Copy it, rename it, build it.
---

# The template

`templates/overcooked-2-addon` in the repository is a working Overcooked 2 addon. Copy it, rename it, then build and install it:

```sh
cp -r templates/overcooked-2-addon ~/party-mode && cd ~/party-mode
mv MyKitchen.csproj PartyMode.csproj
# PartyMode.csproj: <AssemblyName>PartyMode</AssemblyName> and <RootNamespace>PartyMode</RootNamespace>
# swofty.json: "id": "yourname.party-mode", "name": "Party Mode", "assemblies": ["PartyMode.dll"]
dotnet build -c Release -p:SwoftyCoreDir=/path/to/SwoftyLauncher/cores/overcooked_2
swofty --game overcooked_2 addons install dist
```

The template is MIT-licensed; your addon can use any licence.

```text
templates/overcooked-2-addon/
├── swofty.json              id, name, version, compat.game overcooked_2 with the std range, the assembly to load
├── MyKitchen.csproj         a net35 class library against Swofty Core; each build writes the addon folder to dist/
├── src/MyKitchenAddon.cs    the addon
├── README.md                build and install steps
└── LICENSE                  MIT
```

## The addon

<<< @/../templates/overcooked-2-addon/src/MyKitchenAddon.cs{csharp} [src/MyKitchenAddon.cs]

- **`Load`** runs once, when the game loads its first scene: see [Addons and IAddon](/overcooked-2/std/).
- **`MainMenu.AddButton`** adds **MY KITCHEN** to the **MODS** tab, with the id `example.my-kitchen.about`: see [MainMenu](/overcooked-2/std/main-menu).
- **`Game.SceneLoaded`** counts the scenes of the session: see [Game](/overcooked-2/std/game).
- **`AccessTools.TypeByName`** finds the game's `T17FrontendFlow` with no reference to the game's assemblies, and `ctx.Harmony.Patch` adds a postfix to its `Awake`. Without that method, the addon logs a warning and loads without the patch: see [Harmony patches](/overcooked-2/std/harmony).
- **`visits.txt`** is in `ctx.DataDirectory`: see [AddonContext and Log](/overcooked-2/std/addon-context).
- **`Dialogs.Message`** opens the dialog: see [Dialogs](/overcooked-2/std/dialogs).

The second time, **MY KITCHEN** opens:

```text
My Kitchen

My Kitchen 0.1.0
4 scenes loaded this session, 2 visits to this dialog.
```

## swofty.json

```json [swofty.json]
{
  "id": "example.my-kitchen",
  "name": "My Kitchen",
  "version": "0.1.0",
  "author": "Your Name",
  "description": "A starting point for an Overcooked 2 addon: a main menu button, a dialog, scene events, a data folder and a Harmony patch.",
  "license": "MIT",
  "compat": { "game": "overcooked_2", "std": ">=0.1.0" },
  "assemblies": ["MyKitchen.dll"]
}
```

Change `id`, `name`, `author` and `description`, and list your assembly in `assemblies`: see [swofty.json](./manifest).

## The project

```xml [MyKitchen.csproj]
<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <TargetFramework>net35</TargetFramework>
    <AssemblyName>MyKitchen</AssemblyName>
    <RootNamespace>MyKitchen</RootNamespace>
    <Version>0.1.0</Version>
    <LangVersion>latest</LangVersion>
    <Nullable>disable</Nullable>
    <DebugType>none</DebugType>
    <AppendTargetFrameworkToOutputPath>false</AppendTargetFrameworkToOutputPath>
    <SwoftyCoreDir Condition="'$(SwoftyCoreDir)' == ''">$(MSBuildThisFileDirectory)../../games/overcooked_2/dist</SwoftyCoreDir>
    <OC2_DIR Condition="'$(OC2_DIR)' == '' And '$(OS)' == 'Windows_NT'">C:\Program Files (x86)\Steam\steamapps\common\Overcooked! 2</OC2_DIR>
    <OC2_DIR Condition="'$(OC2_DIR)' == ''">$(HOME)/.local/share/Steam/steamapps/common/Overcooked! 2</OC2_DIR>
    <OC2Managed>$(OC2_DIR)/Overcooked2_Data/Managed</OC2Managed>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net35" Version="1.0.3" PrivateAssets="all" />
    <Reference Include="Swofty.Overcooked2" HintPath="$(SwoftyCoreDir)/core/Swofty.Overcooked2.dll" Private="false" />
    <Reference Include="0Harmony" HintPath="$(SwoftyCoreDir)/core/0Harmony.dll" Private="false" />
  </ItemGroup>

  <ItemGroup Condition="Exists('$(OC2Managed)/Assembly-CSharp.dll')">
    <Reference Include="$(OC2Managed)/Assembly-CSharp.dll" Private="false" />
    <Reference Include="$(OC2Managed)/UnityEngine*.dll" Private="false" />
  </ItemGroup>

  <Target Name="SwoftyCheckCore" BeforeTargets="ResolveAssemblyReferences" Condition="!Exists('$(SwoftyCoreDir)/core/Swofty.Overcooked2.dll')">
    <Error Text="Swofty Core not found in $(SwoftyCoreDir). Build it with games/overcooked_2/build.sh or pass -p:SwoftyCoreDir=&lt;launcher&gt;/cores/overcooked_2." />
  </Target>

  <Target Name="SwoftyAddonFolder" AfterTargets="Build">
    <Copy SourceFiles="swofty.json;$(TargetPath)" DestinationFolder="dist" />
  </Target>

</Project>
```

- **`SwoftyCoreDir`**: Swofty Core, with `core/Swofty.Overcooked2.dll` and `core/0Harmony.dll`. Pass `cores/overcooked_2` in the launcher's [install folder](/guide/installation#the-install-folder). In the repository, the default is `games/overcooked_2/dist`, which `games/overcooked_2/build.sh` builds. Without Swofty Core, `SwoftyCheckCore` stops the build with `Swofty Core not found in <folder>`.
- **`OC2_DIR`**: the game folder, by default the Steam library folder on Windows and Linux. With `Assembly-CSharp.dll` there, the project references it and the game's `UnityEngine*.dll`. Without it, the template still builds.
- **`SwoftyAddonFolder`** copies `swofty.json` and the built dll to `dist/` after each build. Install that folder, or a zip of it.

See [Building against the std lib](./building).
