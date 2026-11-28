using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Swofty.Overcooked2.Core;
using Xunit;

[assembly: CollectionBehavior(DisableTestParallelization = true)]

namespace Swofty.Overcooked2.Tests
{
    public static class Probe
    {
        private static readonly List<string> calls = new List<string>();

        public static void Record(string call)
        {
            lock (calls)
            {
                calls.Add(call);
            }
        }

        public static List<string> Calls
        {
            get
            {
                lock (calls)
                {
                    return new List<string>(calls);
                }
            }
        }

        internal static void Reset()
        {
            lock (calls)
            {
                calls.Clear();
            }
        }
    }

    public static class PatchTarget
    {
        [MethodImpl(MethodImplOptions.NoInlining)]
        public static string Value()
        {
            return "original";
        }
    }

    public abstract class HostTest : IDisposable
    {
        protected const string UnityPrefix = "[Swofty] ";

        protected readonly string Root;
        protected readonly string DataDir;
        private protected readonly CoreLog Log;
        protected readonly List<string> Lines = new List<string>();

        protected HostTest()
        {
            ResetStatics();
            Root = Directory.CreateTempSubdirectory("swofty-oc2-tests-").FullName;
            DataDir = Path.Combine(Root, "games", "overcooked_2");
            Directory.CreateDirectory(DataDir);
            Log = new CoreLog();
            Log.Clock = () => new DateTime(2026, 11, 29, 1, 2, 3);
            Log.AttachUnity((level, line) =>
            {
                lock (Lines)
                {
                    Lines.Add(line.StartsWith(UnityPrefix, StringComparison.Ordinal) ? line.Substring(UnityPrefix.Length) : line);
                }
            });
            LogSink.Writer = Log;
        }

        public void Dispose()
        {
            LogSink.Writer = null;
            ResetStatics();
            try
            {
                Directory.Delete(Root, true);
            }
            catch (IOException)
            {
            }
        }

        private static void ResetStatics()
        {
            MainMenu.ResetForTests();
            Game.ResetForTests();
            Dialogs.Backend = null;
            Runtime.SetAddons(new AddonInfo[0]);
            Runtime.LauncherVersion = null;
            Probe.Reset();
        }

        protected string AddonDir(string id)
        {
            return Path.Combine(DataDir, "addons", id);
        }

        protected string BuildAddon(string id, string source)
        {
            string assembly = UniqueName(id);
            Compiler.Build(AddonDir(id), assembly, source);
            return assembly + ".dll";
        }

        protected static string UniqueName(string id)
        {
            return "T" + new string(id.Where(char.IsLetterOrDigit).ToArray()) + "_" + Guid.NewGuid().ToString("N");
        }

        private protected Session SessionWith(params Entry[] entries)
        {
            StringBuilder sb = new StringBuilder();
            sb.Append("{\"schema\": 1, \"launcher\": \"1.10.63\", ");
            sb.Append("\"data_dir\": ").Append(Json.Quote(DataDir)).Append(", ");
            sb.Append("\"addons\": [");
            for (int i = 0; i < entries.Length; i++)
            {
                Entry e = entries[i];
                if (i > 0)
                {
                    sb.Append(", ");
                }
                sb.Append("{\"id\": ").Append(Json.Quote(e.Id));
                sb.Append(", \"name\": ").Append(Json.Quote(e.Name ?? e.Id));
                sb.Append(", \"version\": \"1.2.3\"");
                sb.Append(", \"dir\": ").Append(Json.Quote(e.Dir ?? AddonDir(e.Id)));
                sb.Append(", \"assemblies\": [").Append(string.Join(", ", e.Assemblies.Select(Json.Quote))).Append("]}");
            }
            sb.Append("]}");
            return Session.Parse(sb.ToString());
        }

        private protected List<AddonInfo> LoadAll(Session session)
        {
            return new AddonLoader(Log).LoadAll(session);
        }

        protected List<string> LogLines
        {
            get
            {
                lock (Lines)
                {
                    return new List<string>(Lines);
                }
            }
        }
    }

    public sealed class Entry
    {
        public Entry(string id, params string[] assemblies)
        {
            Id = id;
            Assemblies = assemblies;
        }

        public string Id { get; }

        public string Name { get; set; }

        public string Dir { get; set; }

        public string[] Assemblies { get; }
    }

    internal static class Compiler
    {
        private static readonly Lazy<List<MetadataReference>> references = new Lazy<List<MetadataReference>>(LoadReferences);

        internal static string Build(string dir, string assemblyName, string source)
        {
            Directory.CreateDirectory(dir);
            CSharpCompilation compilation = CSharpCompilation.Create(
                assemblyName,
                new[] { CSharpSyntaxTree.ParseText(source) },
                references.Value,
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
            string path = Path.Combine(dir, assemblyName + ".dll");
            using (FileStream stream = File.Create(path))
            {
                Microsoft.CodeAnalysis.Emit.EmitResult result = compilation.Emit(stream);
                if (!result.Success)
                {
                    throw new InvalidOperationException("the test addon does not compile:\n" + string.Join("\n", result.Diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error)));
                }
            }
            return path;
        }

        private static List<MetadataReference> LoadReferences()
        {
            Dictionary<string, string> byName = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            string tpa = (string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES") ?? string.Empty;
            foreach (string path in tpa.Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
            {
                byName[Path.GetFileName(path)] = path;
            }
            foreach (Type t in new[] { typeof(IAddon), typeof(HarmonyLib.Harmony), typeof(Probe) })
            {
                byName[Path.GetFileName(t.Assembly.Location)] = t.Assembly.Location;
            }
            return byName.Values.Select(p => (MetadataReference)MetadataReference.CreateFromFile(p)).ToList();
        }
    }
}
