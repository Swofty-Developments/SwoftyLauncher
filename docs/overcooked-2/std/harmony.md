---
title: Harmony patches
description: Patch Overcooked 2's own methods with Harmony 2 from an addon, through the Harmony instance in AddonContext. Postfixes, prefixes, patch classes and patching by name.
---

# Harmony patches

Swofty Core ships [Harmony](https://harmony.pardeike.net/) 2. `ctx.Harmony` is a Harmony instance whose id is your addon id. Patch the game's own methods with it.

```csharp [Deliveries.cs]
using HarmonyLib;
using Swofty.Overcooked2;

namespace Deliveries
{
    public sealed class Deliveries : IAddon
    {
        internal static AddonContext Ctx;
        internal static int Delivered;
        internal static int Failed;

        public void Load(AddonContext ctx)
        {
            Ctx = ctx;
            ctx.Harmony.PatchAll(typeof(Deliveries).Assembly);
            MainMenu.AddButton("yourname.deliveries.show", "Deliveries", () =>
                Dialogs.Message("Deliveries", Delivered + " delivered, " + Failed + " wrong dishes."));
        }
    }

    [HarmonyPatch(typeof(ClientOrderControllerBase), nameof(ClientOrderControllerBase.OnFoodDelivered))]
    internal static class CountDeliveries
    {
        private static void Postfix(bool _success)
        {
            if (_success)
                Deliveries.Delivered++;
            else
                Deliveries.Failed++;
        }
    }

    [HarmonyPatch(typeof(ClientOrderControllerBase), nameof(ClientOrderControllerBase.OnOrderExpired))]
    internal static class LogExpiredOrders
    {
        private static void Prefix()
        {
            Deliveries.Ctx.Log.Info("an order expired");
        }
    }
}
```

`PatchAll` applies every `[HarmonyPatch]` class in the assembly. `ClientOrderControllerBase` comes from the game's `Assembly-CSharp.dll`: see [Building against the std lib](/overcooked-2/addons/building#the-game-s-assemblies).

## Patching by name

`AccessTools` finds a game method by name, with no reference to `Assembly-CSharp.dll`:

```csharp
ctx.Harmony.Patch(
    AccessTools.Method("ClientOrderControllerBase:OnFoodDelivered"),
    postfix: new HarmonyMethod(typeof(Deliveries), nameof(AfterDelivery)));

private static void AfterDelivery(bool _success)
{
    if (_success)
        Delivered++;
}
```

## Prefixes, postfixes and arguments

A patch method takes the original's arguments by name, `__instance` for the object, and `__result` for the return value:

```csharp
using OrderController;

[HarmonyPatch(typeof(ClientOrderControllerBase), nameof(ClientOrderControllerBase.OnFoodDelivered))]
internal static class Inspect
{
    private static bool Prefix(ClientOrderControllerBase __instance, bool _success, OrderID _orderID)
    {
        Deliveries.Ctx.Log.Info((_success ? "delivered " : "wrong dish for ") + _orderID);
        return true; // false skips the original and the prefixes after this one
    }
}
```

The [Harmony documentation](https://harmony.pardeike.net/articles/patching.html) covers transpilers, finalizers and priorities.

## When an addon fails

When `Load` throws, Swofty Core calls `UnpatchAll` with your addon id: every patch your addon made is removed, and other addons' patches stay.
