using System.IO;
using Swofty.Overcooked2.Core;
using Xunit;

namespace Swofty.Overcooked2.Tests
{
    public class SessionTests
    {
        private const string ContractExample = @"{
  ""schema"": 1,
  ""launcher"": ""1.10.63"",
  ""game_dir"": ""/abs/game/folder"",
  ""data_dir"": ""/abs/data/games/overcooked_2"",
  ""log"": ""/abs/data/games/overcooked_2/sessions/1/core.log"",
  ""status"": ""/abs/data/games/overcooked_2/sessions/1/status.json"",
  ""addons"": [
    {
      ""id"": ""swofty.hello-kitchen"",
      ""name"": ""Hello Kitchen"",
      ""version"": ""0.1.0"",
      ""dir"": ""/abs/data/games/overcooked_2/addons/swofty.hello-kitchen"",
      ""assemblies"": [""HelloKitchen.dll""]
    }
  ]
}";

        [Fact]
        public void ReadsTheContractExample()
        {
            Session s = Session.Parse(ContractExample);

            Assert.Equal(1, s.Schema);
            Assert.Equal("1.10.63", s.Launcher);
            Assert.Equal("/abs/game/folder", s.GameDir);
            Assert.Equal("/abs/data/games/overcooked_2", s.DataDir);
            Assert.Equal("/abs/data/games/overcooked_2/sessions/1/core.log", s.Log);
            Assert.Equal("/abs/data/games/overcooked_2/sessions/1/status.json", s.Status);
            Assert.Empty(s.Warnings);
            SessionAddon a = Assert.Single(s.Addons);
            Assert.Equal("swofty.hello-kitchen", a.Id);
            Assert.Equal("Hello Kitchen", a.Name);
            Assert.Equal("0.1.0", a.Version);
            Assert.Equal("/abs/data/games/overcooked_2/addons/swofty.hello-kitchen", a.Dir);
            Assert.Equal(new[] { "HelloKitchen.dll" }, a.Assemblies);
            Assert.Null(a.Error);
            Assert.Equal(Path.Combine("/abs/data/games/overcooked_2", "addon-data"), s.AddonDataRoot(a));
        }

        [Fact]
        public void KeepsTheSessionOrder()
        {
            Session s = Session.Parse(@"{""schema"": 1, ""addons"": [
                {""id"": ""c"", ""dir"": ""/a/c""}, {""id"": ""a"", ""dir"": ""/a/a""}, {""id"": ""b"", ""dir"": ""/a/b""}]}");

            Assert.Equal(new[] { "c", "a", "b" }, s.Addons.ConvertAll(a => a.Id));
        }

        [Fact]
        public void FillsDefaultsForOptionalFields()
        {
            Session s = Session.Parse(@"{""schema"": 1, ""addons"": [{""id"": ""x"", ""dir"": ""/d/addons/x""}]}");

            SessionAddon a = Assert.Single(s.Addons);
            Assert.Equal("x", a.Name);
            Assert.Equal("0.0.0", a.Version);
            Assert.Empty(a.Assemblies);
            Assert.Null(s.Launcher);
            Assert.Null(s.Log);
            Assert.Null(s.Status);
            Assert.Equal(Path.Combine("/d", "addon-data"), s.AddonDataRoot(a));
        }

        [Fact]
        public void NoAddonsIsAnEmptyList()
        {
            Assert.Empty(Session.Parse(@"{""schema"": 1}").Addons);
            Assert.Empty(Session.Parse(@"{""schema"": 1, ""addons"": null}").Addons);
        }

        [Theory]
        [InlineData("", "not JSON")]
        [InlineData("{\"schema\": 1,}", "not JSON")]
        [InlineData("[1]", "not a JSON object")]
        [InlineData("{}", "no schema number")]
        [InlineData("{\"schema\": \"1\"}", "no schema number")]
        [InlineData("{\"schema\": 0}", "unknown session schema 0")]
        [InlineData("{\"schema\": 1, \"addons\": {}}", "\"addons\" is not an array")]
        [InlineData("{\"schema\": 1, \"log\": 5}", "\"log\" is not a string")]
        public void RejectsABrokenSession(string text, string message)
        {
            SessionException e = Assert.Throws<SessionException>(() => Session.Parse(text));
            Assert.Contains(message, e.Message);
        }

        [Fact]
        public void PointsAtTheJsonError()
        {
            SessionException e = Assert.Throws<SessionException>(() => Session.Parse("{\n  \"schema\": 1\n  \"addons\": []\n}"));
            Assert.Contains("line 3", e.Message);
        }

        [Fact]
        public void WarnsAboutANewerSchemaAndReadsWhatItKnows()
        {
            Session s = Session.Parse(@"{""schema"": 2, ""launcher"": ""9.0.0"", ""future"": {""x"": [1, 2]}, ""addons"": [{""id"": ""a"", ""dir"": ""/a"", ""later"": true}]}");

            Assert.Equal(2, s.Schema);
            Assert.Equal("9.0.0", s.Launcher);
            Assert.Contains(s.Warnings, w => w.Contains("newer"));
            Assert.Null(Assert.Single(s.Addons).Error);
        }

        [Fact]
        public void ABadEntryFailsOnlyThatAddon()
        {
            Session s = Session.Parse(@"{""schema"": 1, ""addons"": [
                {""id"": ""good.one"", ""dir"": ""/a/good.one"", ""assemblies"": [""A.dll""]},
                {""name"": ""No Id"", ""dir"": ""/a/x""},
                {""id"": ""no.dir""},
                {""id"": ""bad.name"", ""name"": 5, ""dir"": ""/a/bad.name""},
                {""id"": ""bad.assemblies"", ""dir"": ""/a/b"", ""assemblies"": ""A.dll""},
                {""id"": ""empty.assembly"", ""dir"": ""/a/e"", ""assemblies"": [""""]},
                7,
                {""id"": ""good.two"", ""dir"": ""/a/good.two""}
            ]}");

            Assert.Equal(8, s.Addons.Count);
            Assert.Null(s.Addons[0].Error);
            Assert.Equal("addons[1]", s.Addons[1].Id);
            Assert.Contains("no id", s.Addons[1].Error);
            Assert.Contains("no dir", s.Addons[2].Error);
            Assert.Equal("bad.name", s.Addons[3].Id);
            Assert.Contains("\"name\" is not a string", s.Addons[3].Error);
            Assert.Contains("\"assemblies\" is not an array", s.Addons[4].Error);
            Assert.Contains("not a path", s.Addons[5].Error);
            Assert.Equal("addons[6]", s.Addons[6].Id);
            Assert.Contains("not an object", s.Addons[6].Error);
            Assert.Null(s.Addons[7].Error);
        }

        [Fact]
        public void ReadsEscapesAndUnicode()
        {
            Session s = Session.Parse(@"{""schema"": 1, ""game_dir"": ""C:\\Games\\Overcooked! 2"", ""launcher"": ""caf\u00e9 \""x\""""}");

            Assert.Equal(@"C:\Games\Overcooked! 2", s.GameDir);
            Assert.Equal("café \"x\"", s.Launcher);
        }

        [Fact]
        public void LoadRecordsThePathAndReportsAMissingFile()
        {
            string dir = Directory.CreateTempSubdirectory("swofty-oc2-session-").FullName;
            try
            {
                string path = Path.Combine(dir, "session.json");
                File.WriteAllText(path, ContractExample);
                Assert.Equal(path, Session.Load(path).Path);

                SessionException e = Assert.Throws<SessionException>(() => Session.Load(Path.Combine(dir, "missing.json")));
                Assert.Contains("cannot read the session file", e.Message);
            }
            finally
            {
                Directory.Delete(dir, true);
            }
        }
    }
}
