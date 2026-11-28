using System;
using System.Globalization;
using System.IO;
using HarmonyLib;
using Swofty.Overcooked2;

namespace HelloKitchen
{
    public sealed class HelloKitchenAddon : IAddon
    {
        private static AddonContext context;

        public void Load(AddonContext ctx)
        {
            context = ctx;
            ctx.Log.Info(ctx.Name + " " + ctx.Version + " loaded on Overcooked 2 build " + Game.Version + " with Swofty Core " + Runtime.CoreVersion);

            MainMenu.AddButton("swofty.hello-kitchen.hello", "Hello", SayHello);
            MainMenu.Shown += () => ctx.Log.Info("the main menu has shown");
            Game.SceneLoaded += scene => ctx.Log.Info("scene loaded: " + scene);

            Type frontend = AccessTools.TypeByName("T17FrontendFlow");
            if (frontend != null)
            {
                ctx.Harmony.Patch(AccessTools.Method(frontend, "Awake"), postfix: new HarmonyMethod(typeof(HelloKitchenAddon), nameof(FrontendAwake)));
            }
        }

        private static void FrontendAwake()
        {
            context.Log.Info("the front end is up");
        }

        private static void SayHello()
        {
            int count = CountHello();
            context.Log.Info("hello number " + count);
            Dialogs.Message(
                "Hello Kitchen",
                "Hello from " + context.Name + " " + context.Version + "!\n"
                + "You have said hello " + count + (count == 1 ? " time" : " times") + ".\n"
                + "Swofty Core " + Runtime.CoreVersion + ", launcher " + (Runtime.LauncherVersion ?? "none") + ".");
        }

        private static int CountHello()
        {
            string file = Path.Combine(context.DataDirectory, "hello-count.txt");
            int count = 0;
            if (File.Exists(file))
            {
                int.TryParse(File.ReadAllText(file).Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out count);
            }
            count++;
            File.WriteAllText(file, count.ToString(CultureInfo.InvariantCulture));
            return count;
        }
    }
}
