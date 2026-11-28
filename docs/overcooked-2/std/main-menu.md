---
title: MainMenu
description: Add buttons to the Overcooked 2 front-end main menu carousel with MainMenu.AddButton, change their label and visibility, and react when the main menu shows.
---

# MainMenu

`MainMenu.AddButton` adds a button to the front-end main menu carousel, in the game's own button style. It works with the mouse and the pad like the game's buttons.

```csharp [PartyMode.cs]
using Swofty.Overcooked2;

namespace PartyMode
{
    public sealed class PartyMode : IAddon
    {
        private static bool on;

        public void Load(AddonContext ctx)
        {
            MenuButton toggle = null;
            toggle = MainMenu.AddButton("yourname.party-mode.toggle", "Party: off", () =>
            {
                on = !on;
                toggle.Label = on ? "Party: on" : "Party: off";
                ctx.Log.Info("party mode " + (on ? "on" : "off"));
            });

            MenuButton help = MainMenu.AddButton("yourname.party-mode.help", "Party help", () =>
                Dialogs.Message("Party Mode", "Party mode is " + (on ? "on" : "off") + "."));

            MainMenu.Shown += () =>
            {
                help.Visible = on;
                ctx.Log.Info("main menu shown");
            };
        }
    }
}
```

The buttons follow the game's own buttons, in the order addons add them. Swofty Core's **Addons** button comes first.

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

Setting `Label` or `Visible` changes the button in the carousel.

## Shown

```csharp
public static event System.Action Shown;
```

Raised when the main menu has shown. An exception in a handler is logged under your addon id.

```csharp
MainMenu.Shown += () => ctx.Log.Info("main menu shown");
```
