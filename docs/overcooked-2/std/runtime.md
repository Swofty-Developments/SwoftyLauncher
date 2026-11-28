---
title: Runtime
description: The versions of Swofty Core, the standard library and the launcher, and every addon of the session with its state, from Runtime.
---

# Runtime

`Runtime` is Swofty Core and the session: their versions, and every addon with its state.

```csharp [AddonReport.cs]
using System.Text;
using Swofty.Overcooked2;

namespace AddonReport
{
    public sealed class AddonReport : IAddon
    {
        public void Load(AddonContext ctx)
        {
            MainMenu.AddButton("yourname.addon-report.open", "Report", () =>
            {
                StringBuilder text = new StringBuilder();
                text.Append("Swofty Core ").Append(Runtime.CoreVersion)
                    .Append(", std ").Append(Runtime.StdVersion)
                    .Append(", launcher ").Append(Runtime.LauncherVersion ?? "none").Append('\n');
                foreach (AddonInfo addon in Runtime.Addons)
                {
                    text.Append('\n').Append(addon.Name).Append(' ').Append(addon.Version);
                    if (!addon.Loaded)
                        text.Append(": ").Append(addon.Error);
                }
                Dialogs.Message("Addon report", text.ToString());
            });
        }
    }
}
```

## Versions

```csharp
public static string CoreVersion { get; }
public static string StdVersion { get; }
public static string LauncherVersion { get; }
```

- `CoreVersion`: Swofty Core, such as `"0.1.0"`.
- `StdVersion`: `Swofty.Overcooked2.dll`, such as `"0.1.0"`.
- `LauncherVersion`: the launcher that started the session, from the session file, or `null` without one.

## Addons

```csharp
public static System.Collections.Generic.IList<AddonInfo> Addons { get; }

public sealed class AddonInfo
{
    public string Id { get; }
    public string Name { get; }
    public string Version { get; }
    public bool Loaded { get; }
    public string Error { get; }
}
```

Every addon of the session, loaded and failed, in load order. The list is filled once every addon has loaded. `Error` is why the addon failed, or `null` when it loaded.
