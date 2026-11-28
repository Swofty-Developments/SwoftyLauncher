using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Reflection;

namespace Swofty.Overcooked2
{
    /// <summary>Game-styled dialogs.</summary>
    public static class Dialogs
    {
        /// <summary>
        /// Shows a dialog in the game's own style with one OK button. The title and body are shown as given. A call
        /// before the game's dialog system exists waits until it does.
        /// </summary>
        public static void Message(string title, string body)
        {
            IDialogBackend backend = Backend;
            if (backend == null)
            {
                LogSink.Write(LogLevel.Warn, Events.CoreTag, "no dialog system; dropped \"" + title + "\"");
                return;
            }
            backend.Message(title ?? string.Empty, body ?? string.Empty);
        }

        internal static IDialogBackend Backend { get; set; }
    }

    internal interface IDialogBackend
    {
        void Message(string title, string body);
    }

    /// <summary>The running game.</summary>
    public static class Game
    {
        private static string version = "unknown";

        /// <summary>The game's build, for example "66.678012", or "unknown".</summary>
        public static string Version
        {
            get { return version; }
            internal set { version = string.IsNullOrEmpty(value) ? "unknown" : value; }
        }

        /// <summary>A Unity scene has loaded; the argument is the scene name.</summary>
        public static event Action<string> SceneLoaded;

        internal static void RaiseSceneLoaded(string scene)
        {
            Events.Raise(SceneLoaded, scene, "Game.SceneLoaded");
        }

        internal static void RemoveHandlersOwnedBy(string owner)
        {
            SceneLoaded = Events.Without(SceneLoaded, owner);
        }

        internal static void ResetForTests()
        {
            SceneLoaded = null;
            version = "unknown";
        }
    }

    /// <summary>Swofty Core and the addons of this session.</summary>
    public static class Runtime
    {
        private static readonly List<AddonInfo> addons = new List<AddonInfo>();
        private static string coreVersion = "unknown";

        public static string CoreVersion
        {
            get { return coreVersion; }
            internal set { coreVersion = value; }
        }

        public static string StdVersion
        {
            get { return VersionOf(typeof(Runtime).Assembly); }
        }

        /// <summary>The launcher version from the session file, or null without one.</summary>
        public static string LauncherVersion { get; internal set; }

        /// <summary>Every addon of the session, loaded and failed, in session order.</summary>
        public static IList<AddonInfo> Addons
        {
            get
            {
                lock (addons)
                {
                    return new ReadOnlyCollection<AddonInfo>(new List<AddonInfo>(addons));
                }
            }
        }

        internal static void SetAddons(IEnumerable<AddonInfo> infos)
        {
            lock (addons)
            {
                addons.Clear();
                addons.AddRange(infos);
            }
        }

        internal static string VersionOf(Assembly assembly)
        {
            Version v = assembly.GetName().Version;
            return v.Major + "." + v.Minor + "." + v.Build;
        }
    }
}
