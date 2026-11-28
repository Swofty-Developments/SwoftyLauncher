using System;
using System.Globalization;

namespace Swofty.Overcooked2
{
    /// <summary>Writes to the session's core.log under a tag (the addon id) and to Unity's Player.log.</summary>
    public sealed class Log
    {
        private readonly string tag;

        internal Log(string tag)
        {
            this.tag = tag;
        }

        public void Info(string message)
        {
            LogSink.Write(LogLevel.Info, tag, message);
        }

        public void Warn(string message)
        {
            LogSink.Write(LogLevel.Warn, tag, message);
        }

        public void Error(string message)
        {
            LogSink.Write(LogLevel.Error, tag, message);
        }
    }

    internal enum LogLevel
    {
        Info,
        Warn,
        Error,
    }

    internal interface ILogWriter
    {
        void Write(LogLevel level, string tag, string message);
    }

    internal static class LogSink
    {
        private static ILogWriter writer;

        internal static ILogWriter Writer
        {
            get { return writer; }
            set { writer = value; }
        }

        internal static void Write(LogLevel level, string tag, string message)
        {
            ILogWriter w = writer;
            if (w == null)
            {
                Console.Error.WriteLine("[" + tag + "] " + message);
                return;
            }
            w.Write(level, tag, message ?? string.Empty);
        }

        internal static string Format(DateTime time, LogLevel level, string tag, string message)
        {
            string text = message ?? string.Empty;
            if (level == LogLevel.Warn)
            {
                text = "warning: " + text;
            }
            else if (level == LogLevel.Error)
            {
                text = "error: " + text;
            }
            return time.ToString("HH:mm:ss", CultureInfo.InvariantCulture) + " [" + tag + "] " + text;
        }
    }
}
