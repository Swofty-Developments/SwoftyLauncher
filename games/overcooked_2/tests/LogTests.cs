using System;
using System.IO;
using Swofty.Overcooked2.Core;
using Xunit;

namespace Swofty.Overcooked2.Tests
{
    public class LogTests : HostTest
    {
        [Theory]
        [InlineData("Info", "07:08:09 [core] ready")]
        [InlineData("Warn", "07:08:09 [core] warning: ready")]
        [InlineData("Error", "07:08:09 [core] error: ready")]
        public void FormatsLinesLikeTheContract(string level, string expected)
        {
            Assert.Equal(expected, LogSink.Format(new DateTime(2026, 1, 2, 7, 8, 9), Enum.Parse<LogLevel>(level), "core", "ready"));
        }

        [Fact]
        public void AppendsToTheSessionLog()
        {
            string path = Path.Combine(Root, "sessions", "1", "core.log");
            File.WriteAllText(Path.Combine(Root, "unrelated"), string.Empty);
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllText(path, "earlier\n");

            CoreLog log = new CoreLog();
            log.Clock = () => new DateTime(2026, 11, 29, 23, 59, 58);
            log.OpenFile(path);
            log.Info("core line");
            new Log("swofty.hello-kitchen").Info("ignored without a writer");
            LogSink.Writer = log;
            new Log("swofty.hello-kitchen").Warn("addon line");

            Assert.Null(log.OpenError);
            Assert.Equal(new[] { "earlier", "23:59:58 [core] core line", "23:59:58 [swofty.hello-kitchen] warning: addon line" }, ReadShared(path));
        }

        [Fact]
        public void RemembersWhyTheLogCannotOpen()
        {
            string blocker = Path.Combine(Root, "file");
            File.WriteAllText(blocker, string.Empty);

            CoreLog log = new CoreLog();
            log.OpenFile(Path.Combine(blocker, "core.log"));
            log.Info("still fine");

            Assert.StartsWith("cannot open " + Path.Combine(blocker, "core.log"), log.OpenError);
        }

        [Fact]
        public void HandsEarlierLinesToUnityWhenItAttaches()
        {
            CoreLog log = new CoreLog();
            log.Clock = () => new DateTime(2026, 11, 29, 0, 0, 1);
            log.Info("before");
            System.Collections.Generic.List<string> seen = new System.Collections.Generic.List<string>();
            log.AttachUnity((level, line) => seen.Add(level + " " + line));
            log.Error("after");

            Assert.Equal(new[] { "Info [Swofty] 00:00:01 [core] before", "Error [Swofty] 00:00:01 [core] error: after" }, seen);
        }

        private static string[] ReadShared(string path)
        {
            using (FileStream s = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
            using (StreamReader r = new StreamReader(s))
            {
                return r.ReadToEnd().TrimEnd('\n').Split('\n');
            }
        }
    }
}
