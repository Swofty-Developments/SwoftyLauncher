---
title: Game
description: The running Overcooked 2 build with Game.Version, and every Unity scene load with Game.SceneLoaded.
---

# Game

`Game` is the running game: its build, and an event for every scene it loads.

```csharp [SceneTimer.cs]
using System;
using Swofty.Overcooked2;

namespace SceneTimer
{
    public sealed class SceneTimer : IAddon
    {
        private static string current;
        private static DateTime since;

        public void Load(AddonContext ctx)
        {
            ctx.Log.Info("Overcooked 2 " + Game.Version);

            Game.SceneLoaded += scene =>
            {
                if (current != null)
                    ctx.Log.Info(current + " took " + (int)(DateTime.Now - since).TotalSeconds + " s");
                current = scene;
                since = DateTime.Now;
                if (scene == "StartScreen")
                    ctx.Log.Info("back on the main menu");
            };
        }
    }
}
```

## Version

```csharp
public static string Version { get; }
```

The game's build as the game reports it, or `"unknown"`.

## SceneLoaded

```csharp
public static event System.Action<string> SceneLoaded;
```

Raised after Unity loads a scene, with the scene's name. The front end with the main menu is `StartScreen`. An exception in a handler is logged under your addon id.
