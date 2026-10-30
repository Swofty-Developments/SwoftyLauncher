using System.Reflection;
using System.Text;

static class Harness
{
    static Assembly A;

    static Type T(string name)
    {
        var t = A.GetType("F1modding3dtoolV1." + name);
        if (t != null) return t;
        foreach (var x in A.GetTypes()) if (x.FullName.EndsWith("." + name) || x.FullName.EndsWith("+" + name)) return x;
        throw new Exception("type not found: " + name);
    }

    static object Call(string type, string method, params object[] args)
    {
        var t = T(type);
        var ms = t.GetMethods(BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic)
                  .Where(m => m.Name == method && m.GetParameters().Length >= args.Length
                              && m.GetParameters().Skip(args.Length).All(pp => pp.IsOptional))
                  .OrderBy(m => m.GetParameters().Length).ToArray();
        if (ms.Length == 0) throw new Exception($"method not found: {type}.{method}/{args.Length}");
        var full = args.Concat(ms[0].GetParameters().Skip(args.Length).Select(pp => pp.DefaultValue)).ToArray();
        try { return ms[0].Invoke(null, full); }
        catch (TargetInvocationException e) { throw e.InnerException ?? e; }
    }

    static void Set(string type, string member, object value)
    {
        var t = T(type);
        const BindingFlags f = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;
        var p = t.GetProperty(member, f);
        if (p != null) { p.SetValue(null, value); return; }
        var fi = t.GetField(member, f);
        if (fi != null) { fi.SetValue(null, value); return; }
        throw new Exception($"member not found: {type}.{member}");
    }

    static object Get(object o, string member)
    {
        var t = o.GetType();
        const BindingFlags f = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;
        var p = t.GetProperty(member, f);
        if (p != null) return p.GetValue(o);
        var fi = t.GetField(member, f);
        if (fi != null) return fi.GetValue(o);
        return null;
    }

    static void Log(string s) { Console.WriteLine("[harness] " + s); Console.Out.Flush(); }

    static string DetectTeamPrefix(string folder)
    {
        var files = Directory.GetFiles(folder, "*!!!temp000.idf*");
        if (files.Length == 0) return null;
        string fn = Path.GetFileName(files[0]);
        int i = fn.IndexOf("!!!", StringComparison.Ordinal);
        return i > 0 ? fn.Substring(0, i) : null;
    }

    static (int year, string folder) Load(string erp)
    {
        Call("GenericMipmapsSidecarLoader", "ResetPathCache");
        Call("MipmapsSidecarLoader", "ResetPathCache");
        int year = 0;
        try { year = (int)Call("F1YearDetector", "DetectFromErp", erp); } catch (Exception e) { Log("year detect failed: " + e.Message); }
        object located = null;
        try { located = Call("F1PackageLocator", "LocateFromErp", erp); } catch (Exception e) { Log("locate failed: " + e.Message); }
        Set("F1PackageLocator", "Last", located);
        Log($"year={year} located={(located != null)}");
        string folder = (string)Call("ErpIdfExtractor", "ExtractEmbeddedIdfToTempFolder", erp);
        Log("extracted idf folder: " + folder);
        try { Call("GameMaterialCatalog", "LoadFromErp", erp); } catch (Exception e) { Log("material catalog: " + e.Message); }
        Call("F1ModelLoader", "ClearRegistries");
        Call("ModelPartNameRegistry", "Clear");
        if (Environment.GetEnvironmentVariable("HARNESS_SKIP_LOAD") != "1")
        {
            var sw = System.Diagnostics.Stopwatch.StartNew();
            var group = Call("F1ModelLoader", "LoadFolder", folder);
            Log($"F1ModelLoader.LoadFolder done in {sw.Elapsed.TotalSeconds:F1}s, model={(group != null)}");
        }
        return (year, folder);
    }

    static void StageTextures(string idfFolder)
    {
        string root = idfFolder;
        while (root != null && !Path.GetFileName(Path.GetDirectoryName(root) ?? "").Equals("erp_cache", StringComparison.OrdinalIgnoreCase))
            root = Path.GetDirectoryName(root);
        if (root == null) { Log("texture staging: cache root not found"); return; }
        foreach (var old in Directory.EnumerateFiles(root, "*.dds", SearchOption.AllDirectories)) File.Delete(old);
        string texDir = Environment.GetEnvironmentVariable("HARNESS_TEX_DIR");
        if (string.IsNullOrEmpty(texDir)) return;
        if (!Directory.Exists(texDir)) { Log("texture staging: HARNESS_TEX_DIR not found: " + texDir); return; }
        var images = Directory.EnumerateFiles(root, "*!!!temp000.image", SearchOption.AllDirectories).ToList();
        foreach (var dds in Directory.EnumerateFiles(texDir, "*.dds"))
        {
            string stem = Path.GetFileNameWithoutExtension(dds);
            var hits = images.Where(i =>
            {
                string leaf = Path.GetFileName(i);
                string tex = leaf.Substring(0, leaf.IndexOf("!!!", StringComparison.Ordinal));
                return string.Equals(Path.GetFileNameWithoutExtension(tex), stem, StringComparison.OrdinalIgnoreCase);
            }).Select(Path.GetDirectoryName).Distinct().ToList();
            if (hits.Count == 0) { Log($"texture staging: {stem}.dds matches no texture in the ERP; ignored"); continue; }
            foreach (var h in hits)
            {
                File.Copy(dds, Path.Combine(h, stem + ".dds"), true);
                Log($"texture staging: {stem}.dds -> {Path.GetRelativePath(root, h)}");
            }
        }
    }

    static int TyreExport(string carErp, string folder)
    {
        var t = T("CustomTyres");
        var m = t.GetMethod("FindCarTyreSets", BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic);
        var a = new object[] { carErp, null };
        var sets = (System.Collections.IEnumerable)m.Invoke(null, a);
        foreach (var s in sets) Log("tyre set: " + s + "  src=" + Get(s, "SourceErp") + " stem=" + Get(s, "Stem") + " pkg=" + Get(s, "Package") + " wet=" + Get(s, "HasWet"));
        var used = a[1];
        Log("used by this car: " + (used == null ? "(none)" : Get(used, "Stem") + " in " + Get(used, "SourceErp")));
        if (used == null) return 2;
        Action<string> log = x => Log(x);
        var written = Call("TyreGltf", "Export", used, folder, log);
        foreach (var w in (System.Collections.IEnumerable)written) Log("wrote " + w);
        return 0;
    }

    static int Export(string erp, string outGltf)
    {
        var (year, folder) = Load(erp);
        Set("F1FbxExporter", "ExportTyreGeometry", false);
        Set("F1FbxExporter", "PreviewDecalMode", false);
        var sw = System.Diagnostics.Stopwatch.StartNew();
        Call("F1FbxExporter", "ExportCarAllLodsFromErp", erp, outGltf, year);
        Log($"export done in {sw.Elapsed.TotalSeconds:F1}s -> {outGltf}");
        return 0;
    }

    static int Patch(string erp, string gltfFolder, string mode, string cleanErp, string reportPath)
    {
        var (year, folder) = Load(erp);
        string prefix = DetectTeamPrefix(folder);
        StageTextures(folder);
        Log($"team prefix: {prefix}");
        string geomFolder = folder, texFolder = null;
        if (!string.IsNullOrEmpty(cleanErp))
        {
            geomFolder = (string)Call("ErpIdfExtractor", "ExtractEmbeddedIdfToTempFolder", cleanErp, false);
            texFolder = folder;
            Log("clean original geometry folder: " + geomFolder);
        }
        var patcher = T("F1ErpPatcher2021Plus");
        var behaviourT = patcher.GetNestedType("ClassicContentBehaviour", BindingFlags.Public | BindingFlags.NonPublic);
        var modeT = patcher.GetNestedType("NodeAdjustmentMode", BindingFlags.Public | BindingFlags.NonPublic);
        object behaviour = Enum.Parse(behaviourT, "Allow");
        object nodeMode = Enum.Parse(modeT, mode == "full" ? "FullConversion" : "VanillaEdit");
        var sw = System.Diagnostics.Stopwatch.StartNew();
        object rep = Call("F1ErpPatcher2021Plus", "PatchErpInPlace", erp, geomFolder, prefix, gltfFolder, behaviour, nodeMode, year, texFolder);
        bool ok = (bool)Get(rep, "Success");
        var msgs = (System.Collections.IEnumerable)Get(rep, "Messages");
        var lines = new List<string>
        {
            $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] Patch ERP from GLTF -- {(ok ? "SUCCESS" : "FAILED")}",
            $"ERP: {erp}",
            $"glTF folder: {gltfFolder}",
            $"Mode: {nodeMode}  Year: {year}",
            $"Writer log: {Get(rep, "LogPath")}",
            $"LodCountProcessed={Get(rep, "LodCountProcessed")} PartsReplaced={Get(rep, "PartsReplaced")} DescriptorsWritten={Get(rep, "DescriptorsWritten")} SectionsReplaced={Get(rep, "SectionsReplaced")} SectionsPassedThrough={Get(rep, "SectionsPassedThrough")} NewErpFileSize={Get(rep, "NewErpFileSize")}",
            "",
        };
        if (msgs != null) foreach (var m in msgs) lines.Add(m?.ToString());
        if (!string.IsNullOrEmpty(reportPath)) File.WriteAllLines(reportPath, lines);
        foreach (var l in lines) Console.WriteLine(l);
        Log($"patch {(ok ? "SUCCESS" : "FAILED")} in {sw.Elapsed.TotalSeconds:F1}s");
        return ok ? 0 : 2;
    }

    [STAThread]
    static int Main(string[] args)
    {
        try { Console.OutputEncoding = Encoding.UTF8; } catch { }
        string dir = AppContext.BaseDirectory;
        A = Assembly.LoadFrom(Path.Combine(dir, "F1modding3dtoolV1.dll"));
        if (args.Length == 0)
        {
            Console.WriteLine("usage: export <erp> <out.gltf> | patch <erp> <gltfFolder> [full|vanilla] [report.txt] [cleanErp] | extract <erp>");
            return 1;
        }
        try
        {
            switch (args[0])
            {
                case "extract": Log((string)Call("ErpIdfExtractor", "ExtractEmbeddedIdfToTempFolder", args[1])); return 0;
                case "export": return Export(args[1], args[2]);
                case "tyreexport": return TyreExport(args[1], args[2]);
                case "patch": return Patch(args[1], args[2], args.Length > 3 ? args[3] : "full", args.Length > 5 ? args[5] : null, args.Length > 4 ? args[4] : null);
            }
        }
        catch (Exception e) { Console.WriteLine("[harness] EXCEPTION: " + e); return 3; }
        return 1;
    }
}
