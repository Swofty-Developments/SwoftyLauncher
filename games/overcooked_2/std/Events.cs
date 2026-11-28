using System;
using System.Collections.Generic;
using System.Reflection;

namespace Swofty.Overcooked2
{
    internal static class Events
    {
        private static readonly Dictionary<Assembly, string> owners = new Dictionary<Assembly, string>();

        internal const string CoreTag = "core";

        internal static void RegisterOwner(Assembly assembly, string addonId)
        {
            lock (owners)
            {
                owners[assembly] = addonId;
            }
        }

        internal static string OwnerOf(Delegate handler)
        {
            if (handler == null || handler.Method == null || handler.Method.DeclaringType == null)
            {
                return CoreTag;
            }
            lock (owners)
            {
                string id;
                return owners.TryGetValue(handler.Method.DeclaringType.Assembly, out id) ? id : CoreTag;
            }
        }

        internal static Action Without(Action handlers, string owner)
        {
            if (handlers == null)
            {
                return null;
            }
            Action kept = null;
            foreach (Delegate d in handlers.GetInvocationList())
            {
                if (OwnerOf(d) != owner)
                {
                    kept += (Action)d;
                }
            }
            return kept;
        }

        internal static Action<T> Without<T>(Action<T> handlers, string owner)
        {
            if (handlers == null)
            {
                return null;
            }
            Action<T> kept = null;
            foreach (Delegate d in handlers.GetInvocationList())
            {
                if (OwnerOf(d) != owner)
                {
                    kept += (Action<T>)d;
                }
            }
            return kept;
        }

        internal static void Raise(Action handlers, string what)
        {
            if (handlers == null)
            {
                return;
            }
            foreach (Delegate d in handlers.GetInvocationList())
            {
                Invoke((Action)d, what);
            }
        }

        internal static void Raise<T>(Action<T> handlers, T arg, string what)
        {
            if (handlers == null)
            {
                return;
            }
            foreach (Delegate d in handlers.GetInvocationList())
            {
                Action<T> one = (Action<T>)d;
                try
                {
                    one(arg);
                }
                catch (Exception e)
                {
                    LogSink.Write(LogLevel.Error, OwnerOf(one), what + " handler threw: " + Describe(e));
                }
            }
        }

        internal static void Invoke(Action action, string what)
        {
            if (action == null)
            {
                return;
            }
            try
            {
                action();
            }
            catch (Exception e)
            {
                LogSink.Write(LogLevel.Error, OwnerOf(action), what + " threw: " + Describe(e));
            }
        }

        internal static string Describe(Exception e)
        {
            e = Unwrap(e);
            return e.GetType().Name + ": " + e.Message + Environment.NewLine + e.StackTrace;
        }

        internal static string Brief(Exception e)
        {
            e = Unwrap(e);
            return e.GetType().Name + ": " + e.Message;
        }

        private static Exception Unwrap(Exception e)
        {
            while (e is TargetInvocationException && e.InnerException != null)
            {
                e = e.InnerException;
            }
            return e;
        }
    }
}
