using System.Collections.Generic;
using System.IO;
using System.Text;
using Swofty.Overcooked2.Core;
using Xunit;

namespace Swofty.Overcooked2.Tests
{
    public class StatusFileTests : HostTest
    {
        [Fact]
        public void RendersTheContractShape()
        {
            List<AddonInfo> addons = new List<AddonInfo>
            {
                new AddonInfo("swofty.hello-kitchen", "Hello Kitchen", "0.1.0", true, null),
                new AddonInfo("x", "X", "1.0.0", false, "..."),
            };

            string json = StatusFile.Render("0.1.0", "0.1.0", addons, true);

            Assert.Equal("{\"core\": \"0.1.0\", \"std\": \"0.1.0\", \"loaded\": [\"swofty.hello-kitchen\"], \"failed\": [{\"id\": \"x\", \"error\": \"...\"}], \"main_menu\": true}", json);
        }

        [Fact]
        public void RendersNoAddons()
        {
            Assert.Equal("{\"core\": \"1.0.0\", \"std\": \"0.2.0\", \"loaded\": [], \"failed\": [], \"main_menu\": false}", StatusFile.Render("1.0.0", "0.2.0", new List<AddonInfo>(), false));
        }

        [Fact]
        public void EscapesErrorsSoTheFileStaysJson()
        {
            string error = "line one\nline \"two\"\t\\ end \u0001 café";
            string json = StatusFile.Render("0.1.0", "0.1.0", new List<AddonInfo> { new AddonInfo("bad", "Bad", "1.0.0", false, error) }, false);

            Dictionary<string, object> parsed = (Dictionary<string, object>)Json.Parse(json);
            Dictionary<string, object> failed = (Dictionary<string, object>)((List<object>)parsed["failed"])[0];
            Assert.Equal("bad", failed["id"]);
            Assert.Equal(error, failed["error"]);
            Assert.Empty((List<object>)parsed["loaded"]);
            Assert.Equal(false, parsed["main_menu"]);
        }

        [Fact]
        public void WriteCreatesTheFolderAndReplacesTheFile()
        {
            string path = Path.Combine(Root, "sessions", "1", "status.json");

            StatusFile.Write(path, "{\"first\": true}");
            StatusFile.Write(path, "{\"second\": true}");

            byte[] bytes = File.ReadAllBytes(path);
            Assert.Equal("{\"second\": true}\n", Encoding.UTF8.GetString(bytes));
            Assert.NotEqual(0xEF, bytes[0]);
            Assert.False(File.Exists(path + ".tmp"));
        }
    }
}
