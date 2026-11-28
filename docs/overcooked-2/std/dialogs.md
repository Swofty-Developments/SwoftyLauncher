---
title: Dialogs
description: Show a message in the Overcooked 2 dialog style with one OK button with Dialogs.Message.
---

# Dialogs

`Dialogs.Message` shows a dialog in the game's own style with one **OK** button.

```csharp [WhatsNew.cs]
using System.IO;
using Swofty.Overcooked2;

namespace WhatsNew
{
    public sealed class WhatsNew : IAddon
    {
        public void Load(AddonContext ctx)
        {
            string seen = Path.Combine(ctx.DataDirectory, "seen-version.txt");
            bool first = !File.Exists(seen) || File.ReadAllText(seen) != ctx.Version;

            MainMenu.Shown += () =>
            {
                if (!first)
                    return;
                first = false;
                File.WriteAllText(seen, ctx.Version);
                Dialogs.Message(ctx.Name + " " + ctx.Version,
                    "New in this version:\n" +
                    "- a Hello button on the main menu\n" +
                    "- every scene in core.log");
            };
        }
    }
}
```

## Message

```csharp
public static void Message(string title, string body);
```

- `title`: the dialog's title, shown as given.
- `body`: the text. `\n` breaks a line.

`Message` returns at once; the dialog stays until the player selects **OK**. A call before the game's dialog system exists waits until it does.
