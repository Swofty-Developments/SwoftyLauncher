---
title: Building against the std lib
description: Build an Overcooked 2 addon for .NET Framework 3.5 with the .NET SDK, reference Swofty.Overcooked2.dll and 0Harmony.dll without copying them, and reference the game's own assemblies from your Overcooked 2 folder.
---

# Building against the std lib

An addon is a .NET Framework 3.5 class library, the profile of the game's Mono runtime. The .NET 8 SDK builds it on Linux and Windows. This is the [template's](./template) project, for Party Mode:

```xml [PartyMode.csproj]
<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <TargetFramework>net35</TargetFramework>
    <AssemblyName>PartyMode</AssemblyName>
    <RootNamespace>PartyMode</RootNamespace>
    <Version>1.2.0</Version>
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

```sh
dotnet build -c Release -p:SwoftyCoreDir=/path/to/SwoftyLauncher/cores/overcooked_2
swofty --game overcooked_2 addons install dist
```

- **`net35`** with `Microsoft.NETFramework.ReferenceAssemblies.net35`: the .NET Framework 3.5 reference assemblies from NuGet. The build needs no .NET Framework or Mono install.
- **`LangVersion latest`**: current C# compiles to .NET 3.5 IL. Language features that need newer runtime types, such as `async`, `ValueTuple` or `Span`, do not.
- **`Private="false"`** on every reference: Swofty Core and the game ship these assemblies. Your addon folder holds only your own.
- **`SwoftyCoreDir`**: Swofty Core, with `core/Swofty.Overcooked2.dll` and `core/0Harmony.dll`. It is `cores/overcooked_2/` in the launcher's [install folder](/guide/installation#the-install-folder), or `games/overcooked_2/dist/` in the repository after `games/overcooked_2/build.sh`. The default is the repository's, for a project in `templates/`. Without Swofty Core, `SwoftyCheckCore` stops the build.
- **`SwoftyAddonFolder`** copies `swofty.json` and `PartyMode.dll` to `dist/` after each build. Install that folder, or a zip of it.

## The game's assemblies

The game's code is in `Overcooked2_Data/Managed/` in the game folder: `Assembly-CSharp.dll` for the game, `UnityEngine*.dll` for Unity 2017.4. Reference them to patch or call game types directly:

```csharp
[HarmonyPatch(typeof(ClientOrderControllerBase), nameof(ClientOrderControllerBase.OnFoodDelivered))]
internal static class CountDeliveries
{
    private static void Postfix(bool _success) { /* … */ }
}
```

`OC2_DIR` is the game folder, by default the Steam library folder on Windows and Linux. With `Assembly-CSharp.dll` there, the project references it and the game's `UnityEngine*.dll`; without it, the build leaves them out.

```sh
dotnet build -c Release -p:OC2_DIR="$HOME/.local/share/Steam/steamapps/common/Overcooked! 2"
```

Code with no game reference reaches the game by name through `AccessTools`:

```csharp
Type frontend = AccessTools.TypeByName("T17FrontendFlow");
if (frontend != null)
    ctx.Harmony.Patch(AccessTools.Method(frontend, "Awake"), postfix: new HarmonyMethod(typeof(MyAddon), nameof(FrontendAwake)));
```

Never ship the game's assemblies with an addon.
