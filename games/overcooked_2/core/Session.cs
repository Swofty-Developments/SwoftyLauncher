using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;

namespace Swofty.Overcooked2.Core
{
    internal sealed class SessionAddon
    {
        internal string Id;
        internal string Name;
        internal string Version;
        internal string Dir;
        internal List<string> Assemblies = new List<string>();
        internal string Error;
    }

    internal sealed class Session
    {
        internal const int SupportedSchema = 1;

        internal int Schema;
        internal string Path;
        internal string Launcher;
        internal string GameDir;
        internal string DataDir;
        internal string Log;
        internal string Status;
        internal List<SessionAddon> Addons = new List<SessionAddon>();
        internal List<string> Warnings = new List<string>();

        internal string AddonDataRoot(SessionAddon addon)
        {
            if (!string.IsNullOrEmpty(DataDir))
            {
                return System.IO.Path.Combine(DataDir, "addon-data");
            }
            string parent = System.IO.Path.GetDirectoryName(addon.Dir);
            string root = string.IsNullOrEmpty(parent) ? addon.Dir : System.IO.Path.GetDirectoryName(parent);
            return System.IO.Path.Combine(root ?? addon.Dir, "addon-data");
        }

        internal static Session Load(string path)
        {
            string text;
            try
            {
                text = File.ReadAllText(path);
            }
            catch (Exception e)
            {
                throw new SessionException("cannot read the session file " + path + ": " + e.Message);
            }
            Session s = Parse(text);
            s.Path = path;
            return s;
        }

        internal static Session Parse(string text)
        {
            object root;
            try
            {
                root = Json.Parse(text);
            }
            catch (JsonException e)
            {
                throw new SessionException("the session file is not JSON: " + e.Message);
            }
            Dictionary<string, object> obj = root as Dictionary<string, object>;
            if (obj == null)
            {
                throw new SessionException("the session file is not a JSON object");
            }
            Session s = new Session();
            object schema;
            if (!obj.TryGetValue("schema", out schema) || !(schema is double))
            {
                throw new SessionException("the session file has no schema number");
            }
            s.Schema = (int)(double)schema;
            if (s.Schema < 1)
            {
                throw new SessionException("unknown session schema " + s.Schema);
            }
            if (s.Schema > SupportedSchema)
            {
                s.Warnings.Add("session schema " + s.Schema + " is newer than " + SupportedSchema + "; reading the fields this core knows");
            }
            s.Launcher = OptString(obj, "launcher");
            s.GameDir = OptString(obj, "game_dir");
            s.DataDir = OptString(obj, "data_dir");
            s.Log = OptString(obj, "log");
            s.Status = OptString(obj, "status");
            object addons;
            if (obj.TryGetValue("addons", out addons) && addons != null)
            {
                List<object> list = addons as List<object>;
                if (list == null)
                {
                    throw new SessionException("\"addons\" is not an array");
                }
                for (int i = 0; i < list.Count; i++)
                {
                    SessionAddon a;
                    try
                    {
                        a = ParseAddon(list[i], i);
                    }
                    catch (SessionException e)
                    {
                        a = new SessionAddon();
                        Dictionary<string, object> entry = list[i] as Dictionary<string, object>;
                        object id;
                        a.Id = entry != null && entry.TryGetValue("id", out id) && id is string && ((string)id).Length > 0
                            ? (string)id
                            : "addons[" + i.ToString(CultureInfo.InvariantCulture) + "]";
                        a.Name = a.Id;
                        a.Version = "0.0.0";
                        a.Error = e.Message;
                    }
                    s.Addons.Add(a);
                }
            }
            return s;
        }

        private static SessionAddon ParseAddon(object value, int index)
        {
            SessionAddon a = new SessionAddon();
            Dictionary<string, object> obj = value as Dictionary<string, object>;
            if (obj == null)
            {
                a.Id = "addons[" + index.ToString(CultureInfo.InvariantCulture) + "]";
                a.Error = "the session entry is not an object";
                return a;
            }
            a.Id = OptString(obj, "id");
            a.Name = OptString(obj, "name");
            a.Version = OptString(obj, "version");
            a.Dir = OptString(obj, "dir");
            if (string.IsNullOrEmpty(a.Id))
            {
                a.Id = "addons[" + index.ToString(CultureInfo.InvariantCulture) + "]";
                a.Error = "the session entry has no id";
                return a;
            }
            if (string.IsNullOrEmpty(a.Name))
            {
                a.Name = a.Id;
            }
            if (string.IsNullOrEmpty(a.Version))
            {
                a.Version = "0.0.0";
            }
            if (string.IsNullOrEmpty(a.Dir))
            {
                a.Error = "the session entry has no dir";
                return a;
            }
            object assemblies;
            if (obj.TryGetValue("assemblies", out assemblies) && assemblies != null)
            {
                List<object> list = assemblies as List<object>;
                if (list == null)
                {
                    a.Error = "\"assemblies\" is not an array";
                    return a;
                }
                foreach (object item in list)
                {
                    string path = item as string;
                    if (string.IsNullOrEmpty(path))
                    {
                        a.Error = "\"assemblies\" has an entry that is not a path";
                        return a;
                    }
                    a.Assemblies.Add(path);
                }
            }
            return a;
        }

        private static string OptString(Dictionary<string, object> obj, string key)
        {
            object v;
            if (!obj.TryGetValue(key, out v) || v == null)
            {
                return null;
            }
            string s = v as string;
            if (s == null)
            {
                throw new SessionException("\"" + key + "\" is not a string");
            }
            return s;
        }
    }

    internal sealed class SessionException : Exception
    {
        internal SessionException(string message)
            : base(message)
        {
        }
    }
}
