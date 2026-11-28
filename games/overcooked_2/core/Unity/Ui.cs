using System;
using System.Collections;
using System.Reflection;
using HarmonyLib;
using UnityEngine;
using UnityEngine.Events;

namespace Swofty.Overcooked2.Core
{
    internal static class Ui
    {
        private static readonly Type selectableType = AccessTools.TypeByName("UnityEngine.UI.Selectable");
        private static readonly Type buttonType = AccessTools.TypeByName("UnityEngine.UI.Button");
        private static readonly Type navigationType = AccessTools.TypeByName("UnityEngine.UI.Navigation");
        private static readonly Type imageType = AccessTools.TypeByName("UnityEngine.UI.Image");
        private static readonly Type textType = AccessTools.TypeByName("UnityEngine.UI.Text");

        internal static readonly Type T17Button = AccessTools.TypeByName("T17Button");
        internal static readonly Type T17Text = AccessTools.TypeByName("T17Text");
        internal static readonly Type BaseMenuBehaviour = AccessTools.TypeByName("BaseMenuBehaviour");

        internal static bool IsMenuBehaviour(Component c)
        {
            return BaseMenuBehaviour != null && BaseMenuBehaviour.IsInstanceOfType(c);
        }

        internal static bool Ready
        {
            get { return selectableType != null && buttonType != null && navigationType != null && T17Button != null && T17Text != null; }
        }

        internal static object Field(object target, string name)
        {
            FieldInfo f = AccessTools.Field(target.GetType(), name);
            if (f == null)
            {
                throw new MissingFieldException(target.GetType().Name, name);
            }
            return f.GetValue(target);
        }

        internal static void SetField(object target, string name, object value)
        {
            FieldInfo f = AccessTools.Field(target.GetType(), name);
            if (f == null)
            {
                throw new MissingFieldException(target.GetType().Name, name);
            }
            f.SetValue(target, value);
        }

        internal static void SetLabel(GameObject go, string label)
        {
            Component text = go.GetComponentInChildren(T17Text, true);
            if (text != null)
            {
                AccessTools.Method(T17Text, "SetNonLocalizedText").Invoke(text, new object[] { label });
                return;
            }
            Component plain = textType == null ? null : go.GetComponentInChildren(textType, true);
            if (plain != null)
            {
                AccessTools.Property(textType, "text").SetValue(plain, label, null);
            }
        }

        internal static void ReplaceClick(Component button, UnityAction action)
        {
            UnityEvent onClick = (UnityEvent)AccessTools.Property(buttonType, "onClick").GetValue(button, null);
            for (int i = 0; i < onClick.GetPersistentEventCount(); i++)
            {
                onClick.SetPersistentListenerState(i, UnityEventCallState.Off);
            }
            onClick.RemoveAllListeners();
            onClick.AddListener(action);
        }

        internal static void ClearEventTriggers(GameObject go)
        {
            Type trigger = AccessTools.TypeByName("UnityEngine.EventSystems.EventTrigger");
            if (trigger == null)
            {
                return;
            }
            foreach (Component c in go.GetComponentsInChildren(trigger, true))
            {
                UnityEngine.Object.DestroyImmediate(c);
            }
        }

        internal static void SetInteractable(Component selectable, bool value)
        {
            AccessTools.Property(selectableType, "interactable").SetValue(selectable, value, null);
        }

        internal static Component NavigationTarget(Component selectable, string direction)
        {
            object nav = AccessTools.Property(selectableType, "navigation").GetValue(selectable, null);
            return AccessTools.Property(navigationType, direction).GetValue(nav, null) as Component;
        }

        internal static string NavigationMode(Component selectable)
        {
            object nav = AccessTools.Property(selectableType, "navigation").GetValue(selectable, null);
            return AccessTools.Property(navigationType, "mode").GetValue(nav, null).ToString();
        }

        internal static void SetNavigation(Component selectable, string direction, Component target)
        {
            PropertyInfo prop = AccessTools.Property(selectableType, "navigation");
            object nav = prop.GetValue(selectable, null);
            PropertyInfo mode = AccessTools.Property(navigationType, "mode");
            mode.SetValue(nav, Enum.Parse(mode.PropertyType, "Explicit"), null);
            AccessTools.Property(navigationType, direction).SetValue(nav, target, null);
            prop.SetValue(selectable, nav, null);
        }

        internal static void SetBorderTop(Component menu, Component top)
        {
            FieldInfo f = AccessTools.Field(menu.GetType(), "m_BorderSelectables");
            object nav = f.GetValue(menu);
            AccessTools.Property(navigationType, "selectOnUp").SetValue(nav, top, null);
            f.SetValue(menu, nav);
        }

        internal static Sprite GetSprite(Component graphic)
        {
            return imageType == null || graphic == null ? null : AccessTools.Property(imageType, "sprite").GetValue(graphic, null) as Sprite;
        }

        internal static void SetSprite(Component graphic, Sprite sprite)
        {
            if (imageType != null && graphic != null && sprite != null)
            {
                AccessTools.Property(imageType, "sprite").SetValue(graphic, sprite, null);
            }
        }

        internal static Component Image(GameObject go)
        {
            return imageType == null ? null : go.GetComponent(imageType);
        }

        internal static IList MenuList(object frontendRootMenu, int menuType)
        {
            Array editorTypes = (Array)Field(frontendRootMenu, "m_EditorTabAbleMenuTypes");
            object entry = editorTypes.GetValue(menuType);
            return (IList)Field(entry, "menus");
        }
    }
}
