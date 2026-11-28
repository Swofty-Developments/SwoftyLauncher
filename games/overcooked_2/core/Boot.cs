using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;

namespace Doorstop
{
    public static class Entrypoint
    {
        public static void Start()
        {
            try
            {
                Swofty.Overcooked2.Core.Boot.Start();
            }
            catch (Exception e)
            {
                try
                {
                    Console.Error.WriteLine("[Swofty] Swofty Core could not start: " + e);
                }
                catch (Exception)
                {
                }
            }
        }
    }
}

namespace Swofty.Overcooked2.Core
{
    internal static class Boot
    {
        private static readonly List<string> probeDirs = new List<string>();

        internal static string CoreDir { get; private set; }

        internal static void Start()
        {
            string self = Environment.GetEnvironmentVariable("DOORSTOP_INVOKE_DLL_PATH");
            if (string.IsNullOrEmpty(self) || !File.Exists(self))
            {
                self = typeof(Boot).Assembly.Location;
            }
            CoreDir = Path.GetDirectoryName(Path.GetFullPath(self));
            AddProbeDir(CoreDir);
            AppDomain.CurrentDomain.AssemblyResolve += Resolve;
            Run();
        }

        internal static void AddProbeDir(string dir)
        {
            if (string.IsNullOrEmpty(dir))
            {
                return;
            }
            lock (probeDirs)
            {
                if (!probeDirs.Contains(dir))
                {
                    probeDirs.Add(dir);
                }
            }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private static void Run()
        {
            CoreHost host = new CoreHost(CoreDir);
            CoreHost.Instance = host;
            host.Start();
        }

        private static Assembly Resolve(object sender, ResolveEventArgs args)
        {
            string name;
            try
            {
                name = new AssemblyName(args.Name).Name;
            }
            catch (Exception)
            {
                return null;
            }
            foreach (Assembly loaded in AppDomain.CurrentDomain.GetAssemblies())
            {
                if (loaded.GetName().Name == name)
                {
                    return loaded;
                }
            }
            string[] dirs;
            lock (probeDirs)
            {
                dirs = probeDirs.ToArray();
            }
            foreach (string dir in dirs)
            {
                string candidate = Path.Combine(dir, name + ".dll");
                if (File.Exists(candidate))
                {
                    return Assembly.LoadFrom(candidate);
                }
            }
            return null;
        }
    }
}
