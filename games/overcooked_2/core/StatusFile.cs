using System.Collections.Generic;
using System.IO;
using System.Text;

namespace Swofty.Overcooked2.Core
{
    internal static class StatusFile
    {
        internal static string Render(string core, string std, IList<AddonInfo> addons, bool mainMenu)
        {
            JsonWriter w = new JsonWriter();
            w.BeginObject();
            w.Name("core").Value(core);
            w.Name("std").Value(std);
            w.Name("loaded").BeginArray();
            foreach (AddonInfo a in addons)
            {
                if (a.Loaded)
                {
                    w.Value(a.Id);
                }
            }
            w.EndArray();
            w.Name("failed").BeginArray();
            foreach (AddonInfo a in addons)
            {
                if (!a.Loaded)
                {
                    w.BeginObject();
                    w.Name("id").Value(a.Id);
                    w.Name("error").Value(a.Error);
                    w.EndObject();
                }
            }
            w.EndArray();
            w.Name("main_menu").Value(mainMenu);
            w.EndObject();
            return w.ToString();
        }

        internal static void Write(string path, string json)
        {
            string dir = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(dir))
            {
                Directory.CreateDirectory(dir);
            }
            string tmp = path + ".tmp";
            File.WriteAllText(tmp, json + "\n", new UTF8Encoding(false));
            if (File.Exists(path))
            {
                File.Delete(path);
            }
            File.Move(tmp, path);
        }
    }
}
