using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using UnityEngine;

namespace Swofty.Overcooked2.Core
{
    internal static class SceneDump
    {
        internal const string Variable = "SWOFTY_OC2_DUMP";

        private static readonly string target = Environment.GetEnvironmentVariable(Variable);
        private static readonly List<KeyValuePair<float, string>> scheduled = new List<KeyValuePair<float, string>>();
        private static int count;

        internal static bool Enabled
        {
            get { return !string.IsNullOrEmpty(target); }
        }

        internal static void OnSceneLoaded(string scene)
        {
            Schedule(scene);
        }

        internal static void Schedule(string what)
        {
            if (!Enabled)
            {
                return;
            }
            float now = Time.realtimeSinceStartup;
            scheduled.Add(new KeyValuePair<float, string>(now + 3f, what + "+3s"));
            scheduled.Add(new KeyValuePair<float, string>(now + 15f, what + "+15s"));
        }

        internal static void Tick()
        {
            if (!Enabled)
            {
                return;
            }
            if (Time.frameCount % 30 == 0 && File.Exists(target + ".now"))
            {
                File.Delete(target + ".now");
                Write("on request");
            }
            if (scheduled.Count == 0)
            {
                return;
            }
            float now = Time.realtimeSinceStartup;
            for (int i = scheduled.Count - 1; i >= 0; i--)
            {
                if (scheduled[i].Key <= now)
                {
                    string label = scheduled[i].Value;
                    scheduled.RemoveAt(i);
                    Write(label);
                }
            }
        }

        internal static void Write(string label)
        {
            if (!Enabled)
            {
                return;
            }
            try
            {
                StringBuilder sb = new StringBuilder();
                sb.Append("=== dump ").Append(label).Append('\n');
                foreach (GameObject go in Resources.FindObjectsOfTypeAll(typeof(GameObject)))
                {
                    if (go.transform.parent == null && go.scene.IsValid())
                    {
                        Describe(go.transform, 0, sb);
                    }
                }
                count++;
                File.WriteAllText(target + "." + count + ".txt", sb.ToString());
            }
            catch (Exception e)
            {
                CoreHost.Instance.Log.Warn("scene dump failed: " + e.Message);
            }
        }

        internal static string Describe(Transform t, int depth, StringBuilder sb)
        {
            sb.Append(' ', depth * 2).Append(t.gameObject.activeSelf ? "+ " : "- ").Append(t.name);
            RectTransform rt = t as RectTransform;
            if (rt != null)
            {
                sb.Append(" rect=").Append(rt.rect.width.ToString("0")).Append('x').Append(rt.rect.height.ToString("0"));
                sb.Append(" pos=").Append(rt.anchoredPosition.x.ToString("0")).Append(',').Append(rt.anchoredPosition.y.ToString("0"));
                if (rt.localScale != Vector3.one)
                {
                    sb.Append(" scale=").Append(rt.localScale.x.ToString("0.00"));
                }
            }
            sb.Append(" [");
            foreach (Component c in t.GetComponents<Component>())
            {
                if (c == null)
                {
                    sb.Append("null ");
                    continue;
                }
                Type type = c.GetType();
                sb.Append(type.Name);
                PropertyInfo text = type.GetProperty("text", BindingFlags.Public | BindingFlags.Instance);
                if (text != null && text.PropertyType == typeof(string))
                {
                    try
                    {
                        sb.Append("=\"").Append(((string)text.GetValue(c, null) ?? string.Empty).Replace("\n", "\\n")).Append('"');
                    }
                    catch (Exception)
                    {
                    }
                }
                FieldInfo tag = type.GetField("m_LocalizationTag", BindingFlags.Public | BindingFlags.Instance);
                if (tag != null)
                {
                    sb.Append(" tag=").Append(tag.GetValue(c));
                }
                PropertyInfo sprite = type.GetProperty("sprite", BindingFlags.Public | BindingFlags.Instance);
                if (sprite != null)
                {
                    try
                    {
                        UnityEngine.Object s = sprite.GetValue(c, null) as UnityEngine.Object;
                        sb.Append(" sprite=").Append(s == null ? "none" : s.name);
                    }
                    catch (Exception)
                    {
                    }
                }
                sb.Append(' ');
            }
            sb.Append("]\n");
            for (int i = 0; i < t.childCount; i++)
            {
                Describe(t.GetChild(i), depth + 1, sb);
            }
            return sb.ToString();
        }
    }
}
