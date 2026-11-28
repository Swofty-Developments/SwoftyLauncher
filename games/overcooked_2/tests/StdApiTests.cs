using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Xunit;

namespace Swofty.Overcooked2.Tests
{
    public class StdApiTests : HostTest
    {
        private sealed class RecordingMenu : IMainMenuBackend
        {
            internal readonly List<string> Changes = new List<string>();

            public void OnButtonChanged(MenuButton button, MenuChange change)
            {
                Changes.Add(change + " " + button.Id + " " + button.Label + " " + button.Visible);
            }
        }

        private sealed class RecordingDialogs : IDialogBackend
        {
            internal readonly List<string> Shown = new List<string>();

            public void Message(string title, string body)
            {
                Shown.Add(title + "|" + body);
            }
        }

        [Fact]
        public void MenuButtonsKeepTheirOrderAndTellTheMenu()
        {
            RecordingMenu menu = new RecordingMenu();
            MainMenu.Backend = menu;

            MenuButton a = MainMenu.AddButton("a", "Alpha", () => { });
            MenuButton b = MainMenu.AddButton("b", null, () => { });
            a.Label = "Renamed";
            b.Visible = false;
            b.Visible = false;

            Assert.Equal(new[] { "a", "b" }, MainMenu.Snapshot().Select(x => x.Id));
            Assert.Equal("b", b.Label);
            Assert.Equal(new[] { "Added a Alpha True", "Added b b True", "Changed a Renamed True", "Changed b b False" }, menu.Changes);
        }

        [Fact]
        public void MenuButtonsNeedAUniqueIdAndAnAction()
        {
            MainMenu.AddButton("same", "One", () => { });

            Assert.Throws<InvalidOperationException>(() => MainMenu.AddButton("same", "Two", () => { }));
            Assert.Throws<ArgumentException>(() => MainMenu.AddButton("", "Empty", () => { }));
            Assert.Throws<ArgumentNullException>(() => MainMenu.AddButton("no.action", "None", null));
            Assert.Single(MainMenu.Snapshot());
        }

        [Fact]
        public void ClickRunsTheAction()
        {
            int clicks = 0;
            MenuButton button = MainMenu.AddButton("count", "Count", () => clicks++);

            button.Click();
            button.Click();

            Assert.Equal(2, clicks);
        }

        [Fact]
        public void RemovingAnAddonsButtonsTellsTheMenu()
        {
            RecordingMenu menu = new RecordingMenu();
            MainMenu.Backend = menu;
            MainMenu.AddButton("core.one", "Core", () => { });
            using (AddonScope.Enter("some.addon"))
            {
                MainMenu.AddButton("addon.one", "Addon", () => { });
            }

            MainMenu.RemoveOwnedBy("some.addon");

            Assert.Equal(new[] { "core.one" }, MainMenu.Snapshot().Select(x => x.Id));
            Assert.Equal("Removed addon.one Addon True", menu.Changes.Last());
        }

        [Fact]
        public void DialogsGoToTheGameOrAreDroppedWithAWarning()
        {
            Dialogs.Message("Early", "nobody listens");
            Assert.Contains("01:02:03 [core] warning: no dialog system; dropped \"Early\"", LogLines);

            RecordingDialogs dialogs = new RecordingDialogs();
            Dialogs.Backend = dialogs;
            Dialogs.Message("Title", "Body");
            Dialogs.Message(null, null);

            Assert.Equal(new[] { "Title|Body", "|" }, dialogs.Shown);
        }

        [Fact]
        public void GameVersionFallsBackToUnknown()
        {
            Assert.Equal("unknown", Game.Version);
            Game.Version = "66.678012";
            Assert.Equal("66.678012", Game.Version);
            Game.Version = "";
            Assert.Equal("unknown", Game.Version);
        }

        [Fact]
        public void RuntimeReportsVersionsAndAReadOnlyAddonList()
        {
            Runtime.SetAddons(new[] { new AddonInfo("a", "A", "1.0.0", true, null), new AddonInfo("b", "B", "2.0.0", false, "nope") });
            Runtime.LauncherVersion = "1.10.63";

            IList<AddonInfo> addons = Runtime.Addons;

            Assert.Equal("0.1.0", Runtime.StdVersion);
            Assert.Equal("1.10.63", Runtime.LauncherVersion);
            Assert.Equal(new[] { "a 1.0.0 (loaded)", "b 2.0.0 (failed: nope)" }, addons.Select(x => x.ToString()));
            Assert.True(addons.IsReadOnly);
            Assert.Throws<NotSupportedException>(() => addons.Clear());
        }

        [Fact]
        public void TheDataDirectoryIsMadeOnFirstUse()
        {
            AddonContext ctx = new AddonContext("lazy", "Lazy", "1.0.0", Path.Combine(Root, "addons", "lazy"), Path.Combine(Root, "addon-data"));

            Assert.False(Directory.Exists(Path.Combine(Root, "addon-data", "lazy")));
            Assert.False(ctx.HasHarmony);
            Assert.Equal(Path.Combine(Root, "addon-data", "lazy"), ctx.DataDirectory);
            Assert.True(Directory.Exists(ctx.DataDirectory));
            Assert.Equal("lazy", ctx.Harmony.Id);
            Assert.True(ctx.HasHarmony);
        }

        [Fact]
        public void ThePublicApiMatchesTheContract()
        {
            Dictionary<string, string[]> expected = new Dictionary<string, string[]>
            {
                ["IAddon"] = new[] { "Void Load(Swofty.Overcooked2.AddonContext)" },
                ["AddonContext"] = new[] { "DataDirectory", "Directory", "Harmony", "Id", "Log", "Name", "Version" },
                ["Log"] = new[] { "Void Error(System.String)", "Void Info(System.String)", "Void Warn(System.String)" },
                ["MainMenu"] = new[] { "Shown", "Swofty.Overcooked2.MenuButton AddButton(System.String, System.String, System.Action)" },
                ["MenuButton"] = new[] { "Id", "Label", "Visible" },
                ["Dialogs"] = new[] { "Void Message(System.String, System.String)" },
                ["Game"] = new[] { "SceneLoaded", "Version" },
                ["Runtime"] = new[] { "Addons", "CoreVersion", "LauncherVersion", "StdVersion" },
                ["AddonInfo"] = new[] { "Error", "Id", "Loaded", "Name", "Version" },
            };

            Type[] exported = typeof(IAddon).Assembly.GetExportedTypes();

            Assert.Equal(expected.Keys.OrderBy(k => k, StringComparer.Ordinal), exported.Select(t => t.Name).OrderBy(n => n, StringComparer.Ordinal));
            foreach (Type t in exported)
            {
                Assert.Equal("Swofty.Overcooked2", t.Namespace);
                Assert.Equal(expected[t.Name], Members(t));
            }
        }

        private static string[] Members(Type t)
        {
            System.Reflection.BindingFlags flags = System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.DeclaredOnly;
            IEnumerable<string> properties = t.GetProperties(flags).Select(p => p.Name);
            IEnumerable<string> events = t.GetEvents(flags).Select(e => e.Name);
            IEnumerable<string> methods = t.GetMethods(flags).Where(m => !m.IsSpecialName).Select(m => m.ToString());
            if (!t.IsInterface)
            {
                methods = methods.Where(m => m != "System.String ToString()");
            }
            return properties.Concat(events).Concat(methods).OrderBy(n => n, StringComparer.Ordinal).ToArray();
        }
    }
}
