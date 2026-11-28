using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Swofty.Overcooked2.Core;
using Xunit;

namespace Swofty.Overcooked2.Tests
{
    public class AddonLoaderTests : HostTest
    {
        private static string Recorder(string className, string call)
        {
            return @"
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class " + className + @" : IAddon
{
    public void Load(AddonContext ctx) { Probe.Record(""" + call + @""" + "":"" + ctx.Id); }
}";
        }

        [Fact]
        public void LoadsAddonsInSessionOrder()
        {
            string c = BuildAddon("c", Recorder("C", "c"));
            string a = BuildAddon("a", Recorder("A", "a"));
            string b = BuildAddon("b", Recorder("B", "b"));

            List<AddonInfo> infos = LoadAll(SessionWith(new Entry("c", c), new Entry("a", a), new Entry("b", b)));

            Assert.Equal(new[] { "c:c", "a:a", "b:b" }, Probe.Calls);
            Assert.Equal(new[] { "c", "a", "b" }, infos.Select(i => i.Id));
            Assert.All(infos, i => Assert.True(i.Loaded, i.Error));
            Assert.Contains("01:02:03 [core] loaded a 1.2.3", LogLines);
        }

        [Fact]
        public void LoadsAssembliesInTheListedOrder()
        {
            string second = BuildAddon("multi", Recorder("Second", "second"));
            string first = BuildAddon("multi", Recorder("First", "first"));

            List<AddonInfo> infos = LoadAll(SessionWith(new Entry("multi", first, second)));

            Assert.True(Assert.Single(infos).Loaded);
            Assert.Equal(new[] { "first:multi", "second:multi" }, Probe.Calls);
        }

        [Fact]
        public void FindsOnlyPublicConcreteAddonClasses()
        {
            string dll = BuildAddon("discovery", @"
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class One : IAddon { public void Load(AddonContext ctx) { Probe.Record(""one""); } }
public class Two : IAddon { public void Load(AddonContext ctx) { Probe.Record(""two""); } }
public abstract class Base : IAddon { public void Load(AddonContext ctx) { Probe.Record(""abstract""); } }
internal sealed class Hidden : IAddon { public void Load(AddonContext ctx) { Probe.Record(""internal""); } }
public static class Outer { public sealed class Nested : IAddon { public void Load(AddonContext ctx) { Probe.Record(""nested""); } } }
public sealed class NotAnAddon { public void Load(AddonContext ctx) { Probe.Record(""not an addon""); } }
public interface IMore : IAddon { }
");

            List<AddonInfo> infos = LoadAll(SessionWith(new Entry("discovery", dll)));

            Assert.True(Assert.Single(infos).Loaded);
            Assert.Equal(new[] { "nested", "one", "two" }, Probe.Calls.OrderBy(c => c, StringComparer.Ordinal));
        }

        [Fact]
        public void GivesEachAddonItsContext()
        {
            string dll = BuildAddon("ctx.addon", @"
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class Ctx : IAddon
{
    public void Load(AddonContext ctx)
    {
        Probe.Record(ctx.Id + ""|"" + ctx.Name + ""|"" + ctx.Version + ""|"" + ctx.Directory + ""|"" + ctx.DataDirectory + ""|"" + ctx.Harmony.Id);
        ctx.Log.Info(""hello"");
        ctx.Log.Warn(""careful"");
        ctx.Log.Error(""broken"");
    }
}");

            LoadAll(SessionWith(new Entry("ctx.addon", dll) { Name = "Ctx Addon" }));

            string data = Path.Combine(DataDir, "addon-data", "ctx.addon");
            Assert.Equal("ctx.addon|Ctx Addon|1.2.3|" + AddonDir("ctx.addon") + "|" + data + "|ctx.addon", Assert.Single(Probe.Calls));
            Assert.True(Directory.Exists(data));
            Assert.Contains("01:02:03 [ctx.addon] hello", LogLines);
            Assert.Contains("01:02:03 [ctx.addon] warning: careful", LogLines);
            Assert.Contains("01:02:03 [ctx.addon] error: broken", LogLines);
        }

        [Fact]
        public void AThrowingAddonFailsAloneAndIsRolledBack()
        {
            string bad = BuildAddon("bad", @"
using System;
using HarmonyLib;
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class Bad : IAddon
{
    public void Load(AddonContext ctx)
    {
        MainMenu.AddButton(""bad.button"", ""Bad"", () => { });
        MainMenu.Shown += () => Probe.Record(""bad shown"");
        Game.SceneLoaded += s => Probe.Record(""bad scene "" + s);
        ctx.Harmony.Patch(typeof(PatchTarget).GetMethod(""Value""), prefix: new HarmonyMethod(typeof(Bad).GetMethod(""Prefix"")));
        Probe.Record(""while loading: "" + PatchTarget.Value());
        throw new InvalidOperationException(""boom"");
    }

    public static bool Prefix(ref string __result) { __result = ""patched""; return false; }
}");
            string good = BuildAddon("good", @"
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class Good : IAddon
{
    public void Load(AddonContext ctx)
    {
        MainMenu.AddButton(""good.button"", ""Good"", () => Probe.Record(""good clicked""));
        MainMenu.Shown += () => Probe.Record(""good shown"");
        Game.SceneLoaded += s => Probe.Record(""good scene "" + s);
    }
}");

            List<AddonInfo> infos = LoadAll(SessionWith(new Entry("bad", bad), new Entry("good", good)));

            AddonInfo failed = infos[0];
            Assert.False(failed.Loaded);
            Assert.Equal("Bad.Load threw InvalidOperationException: boom", failed.Error);
            Assert.True(infos[1].Loaded, infos[1].Error);
            Assert.Contains("while loading: patched", Probe.Calls);
            Assert.Equal("original", PatchTarget.Value());
            Assert.Equal(new[] { "good.button" }, MainMenu.Snapshot().Select(b => b.Id));
            Assert.Contains(LogLines, l => l.StartsWith("01:02:03 [bad] error: Bad.Load threw InvalidOperationException: boom", StringComparison.Ordinal) && l.Contains("at Bad.Load"));
            Assert.Contains("01:02:03 [core] error: bad failed: Bad.Load threw InvalidOperationException: boom", LogLines);

            Probe.Reset();
            MainMenu.RaiseShown();
            Game.RaiseSceneLoaded("Ident");
            Assert.Equal(new[] { "good shown", "good scene Ident" }, Probe.Calls);
        }

        [Fact]
        public void ReportsEachKindOfBrokenAddon()
        {
            string fine = BuildAddon("fine", Recorder("Fine", "fine"));
            string none = BuildAddon("no.entry", "public sealed class Nothing { }");
            string twice = BuildAddon("twice", Recorder("Twice", "twice"));
            Directory.CreateDirectory(AddonDir("garbage"));
            File.WriteAllText(Path.Combine(AddonDir("garbage"), "Garbage.dll"), "not a dll");
            Directory.CreateDirectory(AddonDir("escape"));
            Directory.CreateDirectory(AddonDir("missing.dll"));
            Directory.CreateDirectory(AddonDir("no.assemblies"));

            List<AddonInfo> infos = LoadAll(SessionWith(
                new Entry("no.folder", "X.dll"),
                new Entry("no.assemblies"),
                new Entry("missing.dll", "Missing.dll"),
                new Entry("escape", "../fine/" + fine),
                new Entry("garbage", "Garbage.dll"),
                new Entry("no.entry", none),
                new Entry("twice", twice),
                new Entry("twice", twice),
                new Entry("fine", fine)));

            Dictionary<int, string> errors = infos.Select((info, i) => (info, i)).ToDictionary(p => p.i, p => p.info.Error);
            Assert.Contains("does not exist", errors[0]);
            Assert.Equal("the addon lists no assemblies", errors[1]);
            Assert.Equal("missing assembly Missing.dll", errors[2]);
            Assert.Equal("assembly path ../fine/" + fine + " is not inside the addon folder", errors[3]);
            Assert.StartsWith("cannot load Garbage.dll: BadImageFormatException", errors[4]);
            Assert.Equal("no public class implements Swofty.Overcooked2.IAddon", errors[5]);
            Assert.Null(errors[6]);
            Assert.Equal("the session lists twice more than once", errors[7]);
            Assert.Null(errors[8]);
            Assert.Equal(new[] { "twice:twice", "fine:fine" }, Probe.Calls);
        }

        [Fact]
        public void KeepsSessionEntryErrors()
        {
            Session session = Session.Parse("{\"schema\": 1, \"addons\": [{\"id\": \"x\"}]}");

            AddonInfo info = Assert.Single(LoadAll(session));

            Assert.False(info.Loaded);
            Assert.Equal("the session entry has no dir", info.Error);
        }

        [Fact]
        public void NoSessionLoadsNothing()
        {
            Assert.Empty(LoadAll(null));
        }

        [Fact]
        public void AThrowingHandlerDoesNotStopTheOthers()
        {
            string loud = BuildAddon("loud", @"
using System;
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class Loud : IAddon
{
    public void Load(AddonContext ctx)
    {
        Game.SceneLoaded += s => { throw new InvalidOperationException(""scene "" + s); };
        MainMenu.Shown += () => { throw new InvalidOperationException(""shown""); };
        MainMenu.AddButton(""loud.button"", ""Loud"", () => { throw new InvalidOperationException(""click""); });
    }
}");
            string quiet = BuildAddon("quiet", @"
using Swofty.Overcooked2;
using Swofty.Overcooked2.Tests;
public sealed class Quiet : IAddon
{
    public void Load(AddonContext ctx)
    {
        Game.SceneLoaded += s => Probe.Record(""scene "" + s);
        MainMenu.Shown += () => Probe.Record(""shown"");
    }
}");

            List<AddonInfo> infos = LoadAll(SessionWith(new Entry("loud", loud), new Entry("quiet", quiet)));
            Assert.All(infos, i => Assert.True(i.Loaded, i.Error));

            Game.RaiseSceneLoaded("Boot");
            MainMenu.RaiseShown();
            MainMenu.Snapshot().Single().Click();

            Assert.Equal(new[] { "scene Boot", "shown" }, Probe.Calls);
            Assert.Contains(LogLines, l => l.StartsWith("01:02:03 [loud] error: Game.SceneLoaded handler threw: InvalidOperationException: scene Boot", StringComparison.Ordinal));
            Assert.Contains(LogLines, l => l.StartsWith("01:02:03 [loud] error: MainMenu.Shown threw: InvalidOperationException: shown", StringComparison.Ordinal));
            Assert.Contains(LogLines, l => l.StartsWith("01:02:03 [loud] error: main menu button loud.button threw: InvalidOperationException: click", StringComparison.Ordinal));
        }

        [Fact]
        public void RecordsWhoOwnsAMenuButton()
        {
            string dll = BuildAddon("owner", @"
using Swofty.Overcooked2;
public sealed class Owner : IAddon { public void Load(AddonContext ctx) { MainMenu.AddButton(""owner.button"", ""Mine"", () => { }); } }");
            MainMenu.AddButton("core.button", "Core", () => { });

            LoadAll(SessionWith(new Entry("owner", dll)));

            Assert.Equal(new[] { null, "owner" }, MainMenu.Snapshot().Select(b => b.Owner));
        }
    }
}
