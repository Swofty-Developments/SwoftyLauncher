---
title: AddonContext and Log
description: AddonContext is what an Overcooked 2 addon knows about itself in Load and afterwards. Its id, name and version, its folder and data folder, its Log and its Harmony instance.
---

# AddonContext and Log

`Load` gets the addon's `AddonContext`. Keep it: it stays valid for the whole session.

```csharp [Notes.cs]
using System;
using System.IO;
using Swofty.Overcooked2;

namespace Notes
{
    public sealed class Notes : IAddon
    {
        public void Load(AddonContext ctx)
        {
            ctx.Log.Info(ctx.Name + " " + ctx.Version + " (" + ctx.Id + ")");
            ctx.Log.Info("from " + ctx.Directory);

            string readme = Path.Combine(ctx.Directory, "notes.txt");
            if (!File.Exists(readme))
            {
                ctx.Log.Warn("notes.txt is missing from the addon folder");
                return;
            }

            string seen = Path.Combine(ctx.DataDirectory, "last-seen.txt");
            string last = File.Exists(seen) ? File.ReadAllText(seen) : "never";
            File.WriteAllText(seen, DateTime.Now.ToString("yyyy-MM-dd HH:mm"));

            string text = File.ReadAllText(readme);
            MainMenu.AddButton(ctx.Id + ".open", "Notes", () => Dialogs.Message(ctx.Name, text + "\n\nLast opened: " + last));
        }
    }
}
```

```text [core.log]
00:15:07 [yourname.notes] Notes 0.1.0 (yourname.notes)
00:15:07 [yourname.notes] from /home/you/.local/share/swofty/games/overcooked_2/addons/yourname.notes
00:15:07 [core] loaded yourname.notes 0.1.0
```

## AddonContext

```csharp
public sealed class AddonContext
{
    public string Id { get; }
    public string Name { get; }
    public string Version { get; }
    public string Directory { get; }
    public string DataDirectory { get; }
    public Log Log { get; }
    public HarmonyLib.Harmony Harmony { get; }
}
```

- `Id`, `Name`, `Version`: from the addon's `swofty.json`.
- `Directory`: the addon's folder, `$DATA/games/overcooked_2/addons/<id>/`. Read your own files from here.
- `DataDirectory`: `$DATA/games/overcooked_2/addon-data/<id>/`, the addon's writable folder, created the first time you read it.
- `Log`: the addon's log, tagged with its id.
- `Harmony`: a [Harmony](./harmony) instance whose id is the addon id, created the first time you read it.

## Log

```csharp
public sealed class Log
{
    public void Info(string message);
    public void Warn(string message);
    public void Error(string message);
}
```

Each call appends one line to the session's `core.log` and to Unity's log, `player.log` in the same folder:

```csharp
ctx.Log.Info("ready");
ctx.Log.Warn("no notes.txt, using the defaults");
ctx.Log.Error("cannot parse settings.json: " + e.Message);
```

```text [core.log]
00:15:07 [yourname.notes] ready
00:15:07 [yourname.notes] warning: no notes.txt, using the defaults
00:15:07 [yourname.notes] error: cannot parse settings.json: Unexpected character at 12
```
