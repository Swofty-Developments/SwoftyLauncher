---
title: Addons and IAddon
description: The Overcooked 2 standard library, Swofty.Overcooked2.dll. An addon is a class that implements IAddon; Swofty Core creates it and calls Load with the addon's context. One addon that uses every part, then each part.
---

# Addons and IAddon

`Swofty.Overcooked2.dll` is the standard library an addon builds against: namespace `Swofty.Overcooked2`, .NET Framework 3.5, the profile of the game's Mono runtime. Swofty Core ships it and loads it before any addon.

An addon is a public class that implements `IAddon`. Swofty Core creates it and calls `Load` once, with the addon's [`AddonContext`](./addon-context).

This addon uses every part of the library. It counts the orders you deliver through a Harmony patch, keeps the count in its data folder, and shows it from a button on the main menu's **MODS** tab.

```csharp [DeliveryCounter.cs]
using System.IO;
using HarmonyLib;
using Swofty.Overcooked2;

namespace DeliveryCounter
{
    public sealed class DeliveryCounter : IAddon
    {
        private static AddonContext ctx;
        private static string file;
        private static int total;
        private static int session;

        public void Load(AddonContext context)
        {
            ctx = context;
            file = Path.Combine(ctx.DataDirectory, "deliveries.txt");
            if (File.Exists(file))
                total = int.Parse(File.ReadAllText(file));

            MenuButton button = MainMenu.AddButton("yourname.delivery-counter.show", "Deliveries", () =>
                Dialogs.Message("Delivery Counter",
                    total + " orders delivered, " + session + " since you started the game.\n" +
                    "Overcooked 2 " + Game.Version + ", Swofty Core " + Runtime.CoreVersion +
                    ", " + Runtime.Addons.Count + " addons."));

            MainMenu.Shown += () => button.Label = "Deliveries (" + total + ")";

            Game.SceneLoaded += scene =>
            {
                ctx.Log.Info("scene " + scene + ", " + total + " deliveries");
                File.WriteAllText(file, total.ToString());
            };

            ctx.Harmony.Patch(
                AccessTools.Method(typeof(ClientOrderControllerBase), nameof(ClientOrderControllerBase.OnFoodDelivered)),
                postfix: new HarmonyMethod(typeof(DeliveryCounter), nameof(AfterDelivery)));
        }

        private static void AfterDelivery(bool _success)
        {
            if (!_success)
                return;
            total++;
            session++;
        }
    }
}
```

```json [swofty.json]
{
  "id": "yourname.delivery-counter",
  "name": "Delivery Counter",
  "version": "0.1.0",
  "author": "Your Name",
  "description": "Counts the orders you deliver.",
  "compat": { "game": "overcooked_2", "std": ">=0.1.0" },
  "assemblies": ["DeliveryCounter.dll"]
}
```

`ClientOrderControllerBase` is the game's own class, from `Assembly-CSharp.dll`: see [Building against the std lib](/overcooked-2/addons/building#the-game-s-assemblies).

## IAddon

```csharp
public interface IAddon
{
    void Load(AddonContext ctx);
}
```

When the game loads its first scene, Swofty Core loads the addons on Unity's main thread. For each addon of the session, in load order, it loads the assemblies named in `swofty.json`, finds every public non-abstract class that implements `IAddon`, creates each one with its parameterless constructor, and calls `Load`. The classes of one addon share one `AddonContext`.

An exception from `Load` fails that addon only. Swofty Core removes the addon's main menu buttons, its `MainMenu.Shown` and `Game.SceneLoaded` handlers and its Harmony patches, then loads the next addon. The error is in `core.log` and `status.json`, and the **Addons** dialog shows the addon as failed:

```text [core.log]
00:15:07 [core] yourname.broken failed: Broken.Broken.Load threw NullReferenceException: Object reference not set to an instance of an object
```

An addon also fails with no assembly, a missing assembly, an assembly path outside its folder, or no class that implements `IAddon`.

An exception in a button's click, a `MainMenu.Shown` handler or a `Game.SceneLoaded` handler is logged under the addon's id. The addon stays loaded.

## The parts

- [AddonContext and Log](./addon-context): the addon's id, folders, log and Harmony instance.
- [MainMenu](./main-menu): buttons on the main menu's **MODS** tab, and when the main menu shows.
- [Dialogs](./dialogs): a message in the game's dialog style.
- [Game](./game): the game's build and every scene load.
- [Runtime](./runtime): the versions of Swofty Core, the standard library and the launcher, and every addon of the session.
- [Harmony patches](./harmony): prefixes and postfixes on the game's own methods.
