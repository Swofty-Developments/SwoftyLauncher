using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text;
using HarmonyLib;
using UnityEngine;

namespace Swofty.Overcooked2.Core
{
    internal sealed class DialogBackend : IDialogBackend
    {
        private static readonly List<KeyValuePair<string, string>> pending = new List<KeyValuePair<string, string>>();

        public void Message(string title, string body)
        {
            lock (pending)
            {
                pending.Add(new KeyValuePair<string, string>(title, body));
            }
        }

        internal static void Pump()
        {
            KeyValuePair<string, string> next;
            lock (pending)
            {
                if (pending.Count == 0)
                {
                    return;
                }
                next = pending[0];
            }
            if (!GameDialogs.TryShow(next.Key, next.Value))
            {
                return;
            }
            lock (pending)
            {
                pending.RemoveAt(0);
            }
        }
    }

    internal static class GameDialogs
    {
        private static Type managerType;
        private static Type dialogType;
        private static MethodInfo getDialog;
        private static MethodInfo initialize;
        private static MethodInfo show;
        private static FieldInfo managerInstance;
        private static Type symbolsType;

        private static bool Resolve()
        {
            if (managerType != null)
            {
                return getDialog != null && initialize != null && show != null;
            }
            managerType = AccessTools.TypeByName("T17DialogBoxManager");
            dialogType = AccessTools.TypeByName("T17DialogBox");
            if (managerType == null || dialogType == null)
            {
                return false;
            }
            managerInstance = AccessTools.Field(managerType, "Instance");
            getDialog = AccessTools.Method(managerType, "GetDialog", new[] { typeof(bool) });
            initialize = AccessTools.Method(dialogType, "Initialize");
            show = AccessTools.Method(dialogType, "Show");
            symbolsType = AccessTools.Inner(dialogType, "Symbols");
            return getDialog != null && initialize != null && show != null;
        }

        internal static bool TryShow(string title, string body)
        {
            if (!Resolve())
            {
                return false;
            }
            if (managerInstance == null || managerInstance.GetValue(null) == null)
            {
                return false;
            }
            object dialog = getDialog.Invoke(null, new object[] { false });
            if (dialog == null)
            {
                return false;
            }
            string legend = Legend.Current();
            object unassigned = Enum.ToObject(symbolsType, 0);
            initialize.Invoke(dialog, new object[] { title, body, "Text.Button.Okay", string.Empty, string.Empty, unassigned, false, false, false });
            if (legend != null)
            {
                FieldInfo onConfirm = AccessTools.Field(dialogType, "OnConfirm");
                FieldInfo onCancel = AccessTools.Field(dialogType, "OnCancel");
                Delegate restore = Delegate.CreateDelegate(onConfirm.FieldType, new Legend(legend), "Restore");
                onConfirm.SetValue(dialog, restore);
                onCancel.SetValue(dialog, restore);
            }
            show.Invoke(dialog, null);
            return true;
        }
    }

    internal sealed class Legend
    {
        private readonly string tag;

        internal Legend(string tag)
        {
            this.tag = tag;
        }

        public void Restore()
        {
            try
            {
                object root = RootMenu();
                if (root != null)
                {
                    AccessTools.Method(root.GetType(), "SetLegendText").Invoke(root, new object[] { tag });
                }
            }
            catch (Exception e)
            {
                CoreHost.Instance.Log.Warn("could not restore the menu legend: " + e.Message);
            }
        }

        internal static string Current()
        {
            try
            {
                object root = RootMenu();
                object text = root == null ? null : Ui.Field(root, "m_t17LegendText");
                return text == null ? null : Ui.Field(text, "m_LocalizationTag") as string;
            }
            catch (Exception)
            {
                return null;
            }
        }

        private static object RootMenu()
        {
            Type flow = AccessTools.TypeByName("T17FrontendFlow");
            PropertyInfo instance = flow == null ? null : AccessTools.Property(flow, "Instance");
            object f = instance == null ? null : instance.GetValue(null, null);
            UnityEngine.Object alive = f as UnityEngine.Object;
            if (f == null || alive == null)
            {
                return null;
            }
            return Ui.Field(f, "m_Rootmenu");
        }
    }

    internal static class AddonsDialog
    {
        internal static void Show()
        {
            StringBuilder sb = new StringBuilder();
            sb.Append("Swofty Core ").Append(Runtime.CoreVersion).Append(" (std ").Append(Runtime.StdVersion).Append(")\n");
            sb.Append("Overcooked 2 build ").Append(Game.Version).Append('\n');
            IList<AddonInfo> addons = Runtime.Addons;
            if (addons.Count == 0)
            {
                sb.Append("\nNo addons are enabled.");
            }
            else
            {
                sb.Append('\n');
                foreach (AddonInfo a in addons)
                {
                    sb.Append(a.Name).Append(' ').Append(a.Version).Append(": ").Append(a.Loaded ? "loaded" : "failed").Append('\n');
                }
            }
            Dialogs.Message("Addons", sb.ToString().TrimEnd('\n'));
        }
    }
}
