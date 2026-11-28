---
title: Building against the std lib
description: Build an Overcooked 2 addon for .NET Framework 3.5 with the .NET SDK, reference Swofty.Overcooked2.dll and 0Harmony.dll without copying them, and reference the game's own assemblies from your Overcooked 2 folder.
---

# Building against the std lib

An addon is a .NET Framework 3.5 class library, the profile of the game's Mono runtime. The .NET 8 SDK builds it on Linux and Windows.

```xml [PartyMode.csproj]
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net35</TargetFramework>
    <AssemblyName>PartyMode</AssemblyName>
    <RootNamespace>PartyMode</RootNamespace>
    <Version>1.2.0</Version>
    <LangVersion>latest</LangVersion>
    <AppendTargetFrameworkToOutputPath>false</AppendTargetFrameworkToOutputPath>
    <SwoftyCore Condition="'$(SwoftyCore)' == ''">$(HOME)/SwoftyLauncher/games/overcooked_2/dist/core</SwoftyCore>
    <OC2_DIR Condition="'$(OC2_DIR)' == ''">$(HOME)/.local/share/Steam/steamapps/common/Overcooked! 2</OC2_DIR>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net35" Version="1.0.3" PrivateAssets="all" />
  </ItemGroup>

  <ItemGroup>
    <Reference Include="Swofty.Overcooked2" HintPath="$(SwoftyCore)/Swofty.Overcooked2.dll" Private="false" />
    <Reference Include="0Harmony" HintPath="$(SwoftyCore)/0Harmony.dll" Private="false" />
  </ItemGroup>

  <ItemGroup Condition="Exists('$(OC2_DIR)/Overcooked2_Data/Managed/Assembly-CSharp.dll')">
    <Reference Include="Assembly-CSharp" HintPath="$(OC2_DIR)/Overcooked2_Data/Managed/Assembly-CSharp.dll" Private="false" />
    <Reference Include="UnityEngine" HintPath="$(OC2_DIR)/Overcooked2_Data/Managed/UnityEngine.dll" Private="false" />
  </ItemGroup>
</Project>
```

```sh
dotnet build -c Release
mkdir -p dist && cp swofty.json bin/Release/PartyMode.dll dist/
swofty --game overcooked_2 addons install dist
```

- **`net35`** with `Microsoft.NETFramework.ReferenceAssemblies.net35`: the .NET Framework 3.5 reference assemblies from NuGet. The build needs no .NET Framework or Mono install.
- **`LangVersion latest`**: current C# compiles to .NET 3.5 IL. Language features that need newer runtime types, such as `async`, `ValueTuple` or `Span`, do not.
- **`Private="false"`** on every reference: Swofty Core and the game ship these assemblies. Your addon folder holds only your own.
- **`SwoftyCore`**: a folder with `Swofty.Overcooked2.dll` and `0Harmony.dll`. Build Swofty Core with `games/overcooked_2/build.sh` in the repository for `games/overcooked_2/dist/core/`, or use `cores/overcooked_2/core/` in the launcher's [install folder](/guide/installation#the-install-folder).

```sh
dotnet build -c Release -p:SwoftyCore=/path/to/SwoftyLauncher/cores/overcooked_2/core
```

## The game's assemblies

The game's code is in `Overcooked2_Data/Managed/` in the game folder: `Assembly-CSharp.dll` for the game, `UnityEngine.dll` for Unity 2017.4. Reference them to patch or call game types directly:

```csharp
[HarmonyPatch(typeof(ClientOrderControllerBase), nameof(ClientOrderControllerBase.OnFoodDelivered))]
internal static class CountDeliveries
{
    private static void Postfix(bool _success) { /* … */ }
}
```

`OC2_DIR` is the game folder. Without it, the condition leaves those references out. Code with no game reference reaches the game by name through `AccessTools`:

```csharp
Type frontend = AccessTools.TypeByName("T17FrontendFlow");
if (frontend != null)
    ctx.Harmony.Patch(AccessTools.Method(frontend, "Awake"), postfix: new HarmonyMethod(typeof(MyAddon), nameof(FrontendAwake)));
```

Never ship the game's assemblies with an addon.
