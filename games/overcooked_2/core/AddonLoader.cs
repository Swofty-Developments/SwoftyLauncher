using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;

namespace Swofty.Overcooked2.Core
{
    internal sealed class AddonLoader
    {
        private readonly CoreLog log;

        internal AddonLoader(CoreLog log)
        {
            this.log = log;
        }

        internal Func<string, Assembly> LoadAssembly = path => Assembly.LoadFrom(path);

        internal Action<string> AddProbeDir = dir => { };

        internal List<AddonInfo> LoadAll(Session session)
        {
            List<AddonInfo> infos = new List<AddonInfo>();
            if (session == null)
            {
                return infos;
            }
            Dictionary<string, bool> seen = new Dictionary<string, bool>();
            foreach (SessionAddon a in session.Addons)
            {
                string error = a.Error;
                if (error == null && seen.ContainsKey(a.Id))
                {
                    error = "the session lists " + a.Id + " more than once";
                }
                seen[a.Id] = true;
                if (error == null)
                {
                    error = LoadOne(a, session);
                }
                string name = string.IsNullOrEmpty(a.Name) ? a.Id : a.Name;
                string version = string.IsNullOrEmpty(a.Version) ? "0.0.0" : a.Version;
                infos.Add(new AddonInfo(a.Id, name, version, error == null, error));
                if (error == null)
                {
                    log.Info("loaded " + a.Id + " " + version);
                }
                else
                {
                    log.Error(a.Id + " failed: " + error);
                }
            }
            return infos;
        }

        private string LoadOne(SessionAddon a, Session session)
        {
            if (!Directory.Exists(a.Dir))
            {
                return "the addon folder " + a.Dir + " does not exist";
            }
            if (a.Assemblies.Count == 0)
            {
                return "the addon lists no assemblies";
            }
            AddProbeDir(a.Dir);
            List<Assembly> assemblies = new List<Assembly>();
            foreach (string rel in a.Assemblies)
            {
                string path = Path.GetFullPath(Path.Combine(a.Dir, rel));
                string root = Path.GetFullPath(a.Dir).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar) + Path.DirectorySeparatorChar;
                if (Path.IsPathRooted(rel) || !path.StartsWith(root, StringComparison.Ordinal))
                {
                    return "assembly path " + rel + " is not inside the addon folder";
                }
                if (!File.Exists(path))
                {
                    return "missing assembly " + rel;
                }
                try
                {
                    Assembly asm = LoadAssembly(path);
                    Events.RegisterOwner(asm, a.Id);
                    assemblies.Add(asm);
                }
                catch (Exception e)
                {
                    return "cannot load " + rel + ": " + e.GetType().Name + ": " + e.Message;
                }
            }
            List<Type> entries = new List<Type>();
            foreach (Assembly asm in assemblies)
            {
                Type[] types;
                try
                {
                    types = asm.GetTypes();
                }
                catch (ReflectionTypeLoadException e)
                {
                    string why = e.LoaderExceptions != null && e.LoaderExceptions.Length > 0 && e.LoaderExceptions[0] != null
                        ? e.LoaderExceptions[0].Message
                        : e.Message;
                    return "cannot read the types of " + asm.GetName().Name + ": " + why;
                }
                foreach (Type t in types)
                {
                    if (t.IsClass && !t.IsAbstract && t.IsVisible && typeof(IAddon).IsAssignableFrom(t))
                    {
                        entries.Add(t);
                    }
                }
            }
            if (entries.Count == 0)
            {
                return "no public class implements Swofty.Overcooked2.IAddon";
            }
            AddonContext ctx = new AddonContext(a.Id, string.IsNullOrEmpty(a.Name) ? a.Id : a.Name, a.Version ?? "0.0.0", a.Dir, session.AddonDataRoot(a));
            using (AddonScope.Enter(a.Id))
            {
                foreach (Type t in entries)
                {
                    try
                    {
                        IAddon addon = (IAddon)Activator.CreateInstance(t);
                        addon.Load(ctx);
                    }
                    catch (Exception e)
                    {
                        log.Write(LogLevel.Error, a.Id, t.FullName + ".Load threw " + Events.Describe(e));
                        RollBack(ctx);
                        return t.FullName + ".Load threw " + Events.Brief(e);
                    }
                }
            }
            return null;
        }

        private void RollBack(AddonContext ctx)
        {
            MainMenu.RemoveOwnedBy(ctx.Id);
            MainMenu.RemoveHandlersOwnedBy(ctx.Id);
            Game.RemoveHandlersOwnedBy(ctx.Id);
            if (ctx.HasHarmony)
            {
                try
                {
                    ctx.Harmony.UnpatchAll(ctx.Id);
                }
                catch (Exception e)
                {
                    log.Warn("could not undo the Harmony patches of " + ctx.Id + ": " + e.Message);
                }
            }
        }
    }
}
