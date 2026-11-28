---
layout: false
title: Swofty Launcher for Overcooked 2
titleTemplate: Addons on Linux
description: Swofty Launcher runs Overcooked 2 with Swofty Core, which loads addons written in C# against a small standard library and patches the game with Harmony.

hero:
  name: Swofty Launcher for Overcooked 2
  tagline: Bringing <em>mod support</em> to
  download:
    windows: https://cdn.swofty.net/launcher/latest/SwoftyLauncherInstaller.exe
    linux: https://cdn.swofty.net/launcher/latest/SwoftyLauncherInstaller-linux.zip
    both: https://cdn.swofty.net/launcher/latest/SwoftyLauncherInstaller.zip
    manual: /guide/installation
  docs: /overcooked-2/guide/

links:
  - { icon: book, text: Documentation, link: /overcooked-2/guide/ }
  - { icon: chef, text: Overcooked 2 on Steam, link: https://store.steampowered.com/app/728880/ }
  - { icon: github, text: Source Code, link: https://github.com/Swofty-Developments/SwoftyLauncher }
  - { icon: tag, text: Releases, link: https://github.com/Swofty-Developments/SwoftyLauncher/releases }

intro: Pick <em>Overcooked 2</em> in the bottom-left of the launcher and it becomes an Overcooked 2 launcher, with its own <em>Library</em> and <em>Addon Browser</em>. <em>Play</em> starts the game with <em>Swofty Core</em>, which loads your addons and adds an <em>Addons</em> button to the main menu.

platforms:
  - { icon: linux, text: Linux, the native Steam build }

featuresTitle: Overcooked 2 in <em>the same launcher</em>
features:
  - icon: layers
    title: Library
    details: Install addons from a <em>zip</em> or a <em>folder</em>, turn them on and off, open their folder and uninstall them.
    link: /overcooked-2/guide/playing#library
  - icon: box
    title: Addon Browser
    details: The same browser as F1 25, fed by the <em>Overcooked 2</em> registry.
    link: /overcooked-2/guide/playing#addon-browser
  - icon: chip
    title: Swofty Core
    details: Loads through <em>Unity Doorstop</em>, patches the game with <em>Harmony</em> and lists every addon under <em>Addons</em> in the main menu.
    link: /overcooked-2/core
  - icon: code
    title: Standard library
    details: Main menu buttons, game-styled dialogs, scene events, versions and a log per addon, in <em>Swofty.Overcooked2.dll</em>.
    link: /overcooked-2/std/
  - icon: sliders
    title: Harmony patches
    details: Each addon gets its own <em>Harmony</em> instance to prefix, postfix or replace any game method.
    link: /overcooked-2/std/harmony
  - icon: file
    title: Logs
    details: Every session writes <em>core.log</em> and <em>status.json</em>, with one tag per addon.
    link: /overcooked-2/core#logs

developers:
  text: An Overcooked 2 addon is a <em>.NET 3.5</em> assembly and a <em>swofty.json</em>. Swofty Core creates each <em>IAddon</em> and calls <em>Load</em> with the addon's context.
  actions:
    - { icon: book, text: Your first addon, link: /overcooked-2/addons/, style: solid }
    - { icon: code, text: Standard library, link: /overcooked-2/std/, style: outline }
    - { icon: chip, text: How the core loads, link: /overcooked-2/core, style: outline }

cta:
  text: Read the <em>installation guide</em> to set up Swofty Launcher, then pick <em>Overcooked 2</em> in the bottom-left.
  actions:
    - { icon: download, text: Installation guide, link: /guide/installation }
    - { icon: book, text: Docs, link: /overcooked-2/guide/ }
    - { icon: github, text: Source, link: https://github.com/Swofty-Developments/SwoftyLauncher }
---

<Landing>
<template #code>

```csharp
using Swofty.Overcooked2;

public sealed class HelloKitchen : IAddon
{
    public void Load(AddonContext ctx)
    {
        MainMenu.AddButton("swofty.hello-kitchen.hello", "Hello", () =>
            Dialogs.Message("Hello Kitchen", "Hello from " + ctx.Name + " " + ctx.Version));

        Game.SceneLoaded += scene => ctx.Log.Info("scene " + scene);
    }
}
```

</template>
</Landing>
