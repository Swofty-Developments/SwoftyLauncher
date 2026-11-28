using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace Swofty.Overcooked2.Core
{
    internal sealed class CoreLog : ILogWriter
    {
        private readonly object gate = new object();
        private readonly List<KeyValuePair<LogLevel, string>> pendingForUnity = new List<KeyValuePair<LogLevel, string>>();
        private StreamWriter file;
        private Action<LogLevel, string> unity;

        internal string FilePath { get; private set; }

        internal string OpenError { get; private set; }

        internal Func<DateTime> Clock = () => DateTime.Now;

        internal void OpenFile(string path)
        {
            if (string.IsNullOrEmpty(path))
            {
                return;
            }
            try
            {
                string dir = Path.GetDirectoryName(path);
                if (!string.IsNullOrEmpty(dir))
                {
                    Directory.CreateDirectory(dir);
                }
                FileStream stream = new FileStream(path, FileMode.Append, FileAccess.Write, FileShare.ReadWrite | FileShare.Delete);
                StreamWriter writer = new StreamWriter(stream, new UTF8Encoding(false));
                writer.AutoFlush = true;
                lock (gate)
                {
                    file = writer;
                    FilePath = path;
                }
            }
            catch (Exception e)
            {
                OpenError = "cannot open " + path + ": " + e.Message;
            }
        }

        internal void AttachUnity(Action<LogLevel, string> sink)
        {
            List<KeyValuePair<LogLevel, string>> pending;
            lock (gate)
            {
                unity = sink;
                pending = new List<KeyValuePair<LogLevel, string>>(pendingForUnity);
                pendingForUnity.Clear();
            }
            foreach (KeyValuePair<LogLevel, string> line in pending)
            {
                SafeUnity(sink, line.Key, line.Value);
            }
        }

        public void Write(LogLevel level, string tag, string message)
        {
            string line = LogSink.Format(Clock(), level, tag, message);
            Action<LogLevel, string> sink;
            lock (gate)
            {
                if (file != null)
                {
                    try
                    {
                        file.WriteLine(line);
                    }
                    catch (Exception)
                    {
                    }
                }
                sink = unity;
                if (sink == null)
                {
                    pendingForUnity.Add(new KeyValuePair<LogLevel, string>(level, line));
                }
            }
            if (sink != null)
            {
                SafeUnity(sink, level, line);
            }
        }

        internal void Info(string message)
        {
            Write(LogLevel.Info, "core", message);
        }

        internal void Warn(string message)
        {
            Write(LogLevel.Warn, "core", message);
        }

        internal void Error(string message)
        {
            Write(LogLevel.Error, "core", message);
        }

        private static void SafeUnity(Action<LogLevel, string> sink, LogLevel level, string line)
        {
            try
            {
                sink(level, "[Swofty] " + line);
            }
            catch (Exception)
            {
            }
        }
    }
}
