using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;

namespace Swofty.Overcooked2.Core
{
    internal sealed class CoreHost
    {
        internal const string SessionVariable = "SWOFTY_OC2_SESSION";

        internal static CoreHost Instance;

        private readonly object gate = new object();
        private bool gameHooked;
        private bool addonsLoaded;
        private bool mainMenuSeen;
        private List<AddonInfo> addons = new List<AddonInfo>();

        internal CoreHost(string coreDir)
        {
            CoreDir = coreDir;
            Log = new CoreLog();
        }

        internal string CoreDir { get; private set; }

        internal CoreLog Log { get; private set; }

        internal Session Session { get; private set; }

        internal string CoreVersion
        {
            get { return Runtime.VersionOf(typeof(CoreHost).Assembly); }
        }

        internal bool MainMenuSeen
        {
            get { return mainMenuSeen; }
        }

        internal void Start()
        {
            LogSink.Writer = Log;
            Runtime.CoreVersion = CoreVersion;
            string sessionPath = Environment.GetEnvironmentVariable(SessionVariable);
            string sessionError = null;
            if (string.IsNullOrEmpty(sessionPath))
            {
                sessionError = SessionVariable + " is not set: no addons load";
            }
            else
            {
                try
                {
                    Session = Session.Load(sessionPath);
                }
                catch (SessionException e)
                {
                    sessionError = e.Message + ": no addons load";
                }
            }
            if (Session != null)
            {
                Log.OpenFile(Session.Log);
                Runtime.LauncherVersion = Session.Launcher;
            }
            Log.Info("Swofty Core " + CoreVersion + " for Overcooked 2 (std " + Runtime.StdVersion + ", Harmony " + HarmonyVersion() + ", Doorstop " + DoorstopVersion() + ")");
            if (Log.OpenError != null)
            {
                Log.Warn(Log.OpenError);
            }
            if (sessionError != null)
            {
                Log.Warn(sessionError);
            }
            else
            {
                Log.Info("session " + Session.Path + " from launcher " + (Session.Launcher ?? "unknown") + ", " + Session.Addons.Count + " addon(s)");
                foreach (string warning in Session.Warnings)
                {
                    Log.Warn(warning);
                }
            }
            AppDomain.CurrentDomain.AssemblyLoad += OnAssemblyLoad;
            foreach (Assembly a in AppDomain.CurrentDomain.GetAssemblies())
            {
                if (IsGameAssembly(a))
                {
                    HookGame();
                }
            }
        }

        private static bool IsGameAssembly(Assembly a)
        {
            return a.GetName().Name == "Assembly-CSharp";
        }

        private void OnAssemblyLoad(object sender, AssemblyLoadEventArgs args)
        {
            if (IsGameAssembly(args.LoadedAssembly))
            {
                HookGame();
            }
        }

        private void HookGame()
        {
            lock (gate)
            {
                if (gameHooked)
                {
                    return;
                }
                gameHooked = true;
            }
            AppDomain.CurrentDomain.AssemblyLoad -= OnAssemblyLoad;
            try
            {
                InstallGameHooks();
            }
            catch (Exception e)
            {
                Log.Error("could not hook the game: " + Events.Describe(e));
            }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void InstallGameHooks()
        {
            GameHooks.Install(this);
        }

        internal void LoadAddons()
        {
            lock (gate)
            {
                if (addonsLoaded)
                {
                    return;
                }
                addonsLoaded = true;
            }
            AddonLoader loader = new AddonLoader(Log);
            loader.AddProbeDir = Boot.AddProbeDir;
            addons = loader.LoadAll(Session);
            Runtime.SetAddons(addons);
            int failed = addons.FindAll(a => !a.Loaded).Count;
            Log.Info("addons: " + (addons.Count - failed) + " loaded, " + failed + " failed");
            WriteStatus();
        }

        internal void OnMainMenuShown()
        {
            bool first = false;
            lock (gate)
            {
                if (!mainMenuSeen)
                {
                    mainMenuSeen = true;
                    first = true;
                }
            }
            if (first)
            {
                Log.Info("main menu shown");
                WriteStatus();
            }
            MainMenu.RaiseShown();
        }

        internal void WriteStatus()
        {
            if (Session == null || string.IsNullOrEmpty(Session.Status))
            {
                return;
            }
            try
            {
                StatusFile.Write(Session.Status, StatusFile.Render(CoreVersion, Runtime.StdVersion, addons, mainMenuSeen));
            }
            catch (Exception e)
            {
                Log.Warn("cannot write " + Session.Status + ": " + e.Message);
            }
        }

        private static string HarmonyVersion()
        {
            try
            {
                return Runtime.VersionOf(typeof(HarmonyLib.Harmony).Assembly);
            }
            catch (Exception)
            {
                return "unknown";
            }
        }

        private string DoorstopVersion()
        {
            try
            {
                string path = Path.Combine(Path.GetDirectoryName(CoreDir) ?? CoreDir, "core.json");
                if (File.Exists(path))
                {
                    Dictionary<string, object> obj = Json.Parse(File.ReadAllText(path)) as Dictionary<string, object>;
                    object v;
                    if (obj != null && obj.TryGetValue("doorstop", out v) && v is string)
                    {
                        return (string)v;
                    }
                }
            }
            catch (Exception)
            {
            }
            return "unknown";
        }
    }
}
