---
title: MainMenu
description: Add buttons to the MODS tab of the Overcooked 2 main menu with MainMenu.AddButton, change their label and visibility, and react when the main menu shows.
---

# MainMenu

Swofty Core adds a **MODS** tab to the front-end main menu, after **SETTINGS**, in the game's own style. `MainMenu.AddButton` adds a button to it. The buttons work with the mouse, the keyboard and the pad like the game's own.

```csharp [PartyMode.cs]
using Swofty.Overcooked2;

namespace PartyMode
{
    public sealed class PartyMode : IAddon
    {
        private static bool on;
        private static int shown;

        public void Load(AddonContext ctx)
        {
            MenuButton help = MainMenu.AddButton("yourname.party-mode.help", "Party help", () =>
                Dialogs.Message("Party Mode", "Party mode is on."));
            help.Visible = false;

            MenuButton toggle = null;
            toggle = MainMenu.AddButton("yourname.party-mode.toggle", "Party: off", () =>
            {
                on = !on;
                toggle.Label = on ? "Party: on" : "Party: off";
                help.Visible = on;
                ctx.Log.Info("party mode " + (on ? "on" : "off"));
            });

            MainMenu.Shown += () => ctx.Log.Info("main menu shown " + (++shown) + " times");
        }
    }
}
```

Swofty Core's **Addons** button comes first, then the addons' buttons in the order they are added:

```text
MODS
  ADDONS
  PARTY: OFF
```

Selecting **PARTY: OFF** turns it into **PARTY: ON** and shows **PARTY HELP** above it. The game draws the labels in capitals.

## AddButton

```csharp
public static MenuButton AddButton(string id, string label, System.Action onClick);
```

- `id`: unique across every addon. Start it with your addon id.
- `label`: the text on the button, shown as given. It is not looked up as a localisation key.
- `onClick`: runs when the player selects the button. An exception in it is logged under your addon id.

`AddButton` throws `ArgumentException` for an empty `id`, `ArgumentNullException` for a null `onClick`, and `InvalidOperationException` when a button with that `id` exists.

## MenuButton

```csharp
public sealed class MenuButton
{
    public string Id { get; }
    public string Label { get; set; }
    public bool Visible { get; set; }
}
```

Setting `Label` or `Visible` changes the button on the **MODS** tab.

## Shown

```csharp
public static event System.Action Shown;
```

Raised every time the main menu shows. An exception in a handler is logged under your addon id.

```csharp
MainMenu.Shown += () => ctx.Log.Info("main menu shown");
```
