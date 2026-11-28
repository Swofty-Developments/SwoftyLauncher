using System;
using System.IO;
using System.Reflection;
using HarmonyLib;
using Swofty.Overcooked2;

namespace MyKitchen
{
    public sealed class MyKitchenAddon : IAddon
    {
        private static AddonContext context;
        private static int scenes;

        public void Load(AddonContext ctx)
        {
            context = ctx;
            ctx.Log.Info(ctx.Name + " " + ctx.Version + " on Overcooked 2 " + Game.Version + ", Swofty Core " + Runtime.CoreVersion);

            MainMenu.AddButton(ctx.Id + ".about", "My Kitchen", ShowAbout);
            MainMenu.Shown += () => ctx.Log.Info("the main menu has shown");
            Game.SceneLoaded += scene =>
            {
                scenes++;
                ctx.Log.Info("scene " + scene);
            };

            Type flow = AccessTools.TypeByName("T17FrontendFlow");
            MethodInfo awake = flow == null ? null : AccessTools.Method(flow, "Awake");
            if (awake == null)
            {
                ctx.Log.Warn("T17FrontendFlow.Awake is not in this build; the patch is off");
                return;
            }
            ctx.Harmony.Patch(awake, postfix: new HarmonyMethod(typeof(MyKitchenAddon), nameof(AfterFrontendAwake)));
        }

        private static void AfterFrontendAwake()
        {
            context.Log.Info("the front end is up");
        }

        private static void ShowAbout()
        {
            string visits = Path.Combine(context.DataDirectory, "visits.txt");
            int count = File.Exists(visits) ? int.Parse(File.ReadAllText(visits).Trim()) + 1 : 1;
            File.WriteAllText(visits, count.ToString());

            Dialogs.Message(context.Name, context.Name + " " + context.Version + "\n" + Plural(scenes, "scene") + " loaded this session, " + Plural(count, "visit") + " to this dialog.");
        }

        private static string Plural(int n, string word)
        {
            return n + " " + (n == 1 ? word : word + "s");
        }
    }
}
