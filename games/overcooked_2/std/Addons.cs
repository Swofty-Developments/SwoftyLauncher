using System;
using System.IO;
using HarmonyLib;

namespace Swofty.Overcooked2
{
    /// <summary>
    /// An addon's entry point. Swofty Core creates every public non-abstract class that implements it in an
    /// addon's assemblies and calls <see cref="Load"/> once, addons in session order. An exception fails that
    /// addon only.
    /// </summary>
    public interface IAddon
    {
        void Load(AddonContext ctx);
    }

    /// <summary>What an addon knows about itself while it loads and afterwards.</summary>
    public sealed class AddonContext
    {
        private readonly string dataRoot;
        private string dataDirectory;
        private Harmony harmony;

        internal AddonContext(string id, string name, string version, string directory, string dataRoot)
        {
            Id = id;
            Name = name;
            Version = version;
            Directory = directory;
            this.dataRoot = dataRoot;
            Log = new Log(id);
        }

        public string Id { get; private set; }

        public string Name { get; private set; }

        public string Version { get; private set; }

        /// <summary>The addon's folder.</summary>
        public string Directory { get; private set; }

        /// <summary>The addon's writable folder, created on first use.</summary>
        public string DataDirectory
        {
            get
            {
                if (dataDirectory == null)
                {
                    string path = Path.Combine(dataRoot, Id);
                    System.IO.Directory.CreateDirectory(path);
                    dataDirectory = path;
                }
                return dataDirectory;
            }
        }

        public Log Log { get; private set; }

        /// <summary>A Harmony instance whose id is the addon id.</summary>
        public Harmony Harmony
        {
            get
            {
                if (harmony == null)
                {
                    harmony = new Harmony(Id);
                }
                return harmony;
            }
        }

        internal bool HasHarmony
        {
            get { return harmony != null; }
        }

        internal string DataRoot
        {
            get { return dataRoot; }
        }
    }

    /// <summary>An addon as the core loaded it, or tried to.</summary>
    public sealed class AddonInfo
    {
        internal AddonInfo(string id, string name, string version, bool loaded, string error)
        {
            Id = id;
            Name = name;
            Version = version;
            Loaded = loaded;
            Error = error;
        }

        public string Id { get; private set; }

        public string Name { get; private set; }

        public string Version { get; private set; }

        public bool Loaded { get; private set; }

        /// <summary>Why the addon failed, or null when it loaded.</summary>
        public string Error { get; private set; }

        public override string ToString()
        {
            return Loaded ? Id + " " + Version + " (loaded)" : Id + " " + Version + " (failed: " + Error + ")";
        }
    }

    internal static class AddonScope
    {
        [ThreadStatic]
        private static string current;

        internal static string Current
        {
            get { return current; }
        }

        internal static IDisposable Enter(string id)
        {
            return new Scope(id);
        }

        private sealed class Scope : IDisposable
        {
            private readonly string previous;

            internal Scope(string id)
            {
                previous = current;
                current = id;
            }

            public void Dispose()
            {
                current = previous;
            }
        }
    }
}
