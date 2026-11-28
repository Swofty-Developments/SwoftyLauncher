using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using HarmonyLib;
using UnityEngine;

namespace Swofty.Overcooked2.Core
{
    internal sealed class MenuInjector : IMainMenuBackend
    {
        internal const string TabLabel = "MODS";
        private const int MainMenuType = 0;
        private const int TicketRows = 3;

        internal static readonly MenuInjector Instance = new MenuInjector();

        private static CoreHost host;
        private static LiveMenu live;

        internal static void Install(Harmony harmony, CoreHost coreHost)
        {
            host = coreHost;
            Type frontendRootMenu = AccessTools.TypeByName("FrontendRootMenu");
            if (frontendRootMenu == null || !Ui.Ready)
            {
                host.Log.Error("the game has no FrontendRootMenu or uGUI types; the main menu buttons are off");
                return;
            }
            harmony.Patch(AccessTools.Method(frontendRootMenu, "Awake"), prefix: new HarmonyMethod(typeof(MenuInjector), nameof(FrontendAwakePrefix)));
            harmony.Patch(AccessTools.Method(frontendRootMenu, "Show"), postfix: new HarmonyMethod(typeof(MenuInjector), nameof(FrontendShowPostfix)));
        }

        public void OnButtonChanged(MenuButton button, MenuChange change)
        {
            CoreBehaviour.Post(() =>
            {
                if (live != null && live.Alive)
                {
                    Sync(live);
                }
            });
        }

        private static void FrontendAwakePrefix(MonoBehaviour __instance)
        {
            try
            {
                live = Build(__instance);
                host.Log.Info("added the " + TabLabel + " tab to the main menu with " + live.Entries.Count + " button(s)");
            }
            catch (Exception e)
            {
                live = null;
                host.Log.Error("could not add the " + TabLabel + " tab to the main menu: " + Events.Describe(e));
            }
        }

        private static void FrontendShowPostfix(bool __result)
        {
            if (__result)
            {
                SceneDump.Schedule("main menu");
                host.OnMainMenuShown();
            }
        }

        private static LiveMenu Build(MonoBehaviour root)
        {
            Component panel = Ui.Field(root, "m_MainTabPanel") as Component;
            if (panel == null)
            {
                throw new InvalidOperationException("FrontendRootMenu has no main tab panel");
            }
            List<Component> tabs = new List<Component>();
            for (int i = 0; i < panel.transform.childCount; i++)
            {
                Component b = panel.transform.GetChild(i).GetComponent(Ui.T17Button);
                if (b != null)
                {
                    tabs.Add(b);
                }
            }
            IList menus = Ui.MenuList(root, MainMenuType);
            if (tabs.Count < 2 || menus.Count < tabs.Count)
            {
                throw new InvalidOperationException("the main menu has " + tabs.Count + " tabs and " + menus.Count + " menus");
            }
            Component lastTab = tabs[tabs.Count - 1];
            Component lastBody = (Component)menus[tabs.Count - 1];

            LiveMenu m = new LiveMenu();
            m.Tab = UnityEngine.Object.Instantiate(lastTab.gameObject, lastTab.transform.parent, false);
            m.Tab.name = "SwoftyModsTab";
            m.Tab.transform.SetSiblingIndex(lastTab.transform.GetSiblingIndex() + 1);
            m.TabButton = m.Tab.GetComponent(Ui.T17Button);
            m.PreviousTab = lastTab;
            host.Log.Info("main menu navigation: tabs " + Ui.NavigationMode(lastTab) + ", entries " + DescribeEntryNavigation(lastBody));
            foreach (Component entry in lastBody.GetComponentsInChildren(Ui.T17Button, true))
            {
                if (Ui.NavigationMode(entry) == "Explicit" && Ui.NavigationTarget(entry, "selectOnRight") == null)
                {
                    Ui.SetNavigation(entry, "selectOnRight", m.TabButton);
                }
            }
            if (Ui.NavigationMode(lastTab) == "Explicit")
            {
                Ui.SetNavigation(lastTab, "selectOnRight", m.TabButton);
                Ui.SetNavigation(m.TabButton, "selectOnLeft", lastTab);
                Ui.SetNavigation(m.TabButton, "selectOnRight", null);
            }
            Ui.SetLabel(m.Tab, TabLabel);
            Ui.SetSprite(Ui.Image(m.Tab), Ui.GetSprite(Ui.Image(tabs[1].gameObject)));

            m.Body = UnityEngine.Object.Instantiate(lastBody.gameObject, lastBody.transform.parent, false);
            m.Body.name = "SwoftyModsOptions";
            m.Body.transform.SetSiblingIndex(lastBody.transform.GetSiblingIndex() + 1);
            m.BodyMenu = m.Body.GetComponent(lastBody.GetType());
            PrepareBody(m, panel, tabs.Count + 1);

            tabs.Add(m.TabButton);
            List<Component> bodies = new List<Component>();
            for (int i = 0; i < tabs.Count - 1; i++)
            {
                bodies.Add((Component)menus[i]);
            }
            bodies.Add(m.BodyMenu);
            FitTabs(tabs, bodies);

            menus.Insert(tabs.Count - 1, m.BodyMenu);
            ExtendMenuBodies(panel, tabs.Count - 1, m.BodyMenu);
            Sync(m);
            return m;
        }

        private static void PrepareBody(LiveMenu m, Component panel, int ticketNumber)
        {
            List<GameObject> buttons = new List<GameObject>();
            for (int i = 0; i < m.Body.transform.childCount; i++)
            {
                Transform child = m.Body.transform.GetChild(i);
                if (child.GetComponent(Ui.T17Button) != null)
                {
                    buttons.Add(child.gameObject);
                }
                else if (child.name == "TicketNumber")
                {
                    m.TicketNumber = child.gameObject;
                }
            }
            if (buttons.Count == 0)
            {
                throw new InvalidOperationException("the last main menu ticket has no buttons to copy");
            }
            m.EntryTemplate = buttons[0];
            m.EntryTemplate.name = "SwoftyEntryTemplate";
            m.EntryTemplate.SetActive(false);
            Ui.ClearEventTriggers(m.EntryTemplate);
            for (int i = 1; i < buttons.Count; i++)
            {
                UnityEngine.Object.DestroyImmediate(buttons[i]);
            }
            foreach (MonoBehaviour c in m.Body.GetComponents<MonoBehaviour>())
            {
                if (c != m.BodyMenu && Ui.IsMenuBehaviour(c))
                {
                    UnityEngine.Object.DestroyImmediate(c);
                }
            }
            Transform empty = FindEmptyRow(panel.transform);
            if (empty != null)
            {
                m.SpacerTemplate = UnityEngine.Object.Instantiate(empty.gameObject, m.Body.transform, false);
                m.SpacerTemplate.name = "SwoftySpacerTemplate";
                m.SpacerTemplate.SetActive(false);
            }
            if (m.TicketNumber != null)
            {
                Ui.SetLabel(m.TicketNumber, ticketNumber.ToString("00", CultureInfo.InvariantCulture));
            }
            Component image = Ui.Image(m.Body);
            m.BodySprite = Ui.GetSprite(image);
            m.BodyHeight = ((RectTransform)m.Body.transform).sizeDelta.y;
            m.TallBodySprite = FindTallTicketSprite(panel.transform);
        }

        private static string DescribeEntryNavigation(Component body)
        {
            Component first = body.GetComponentInChildren(Ui.T17Button, true);
            if (first == null)
            {
                return "none";
            }
            return Ui.NavigationMode(first) + " (up " + Name(Ui.NavigationTarget(first, "selectOnUp")) + ", left " + Name(Ui.NavigationTarget(first, "selectOnLeft"))
                + ", right " + Name(Ui.NavigationTarget(first, "selectOnRight")) + ")";
        }

        private static string Name(Component c)
        {
            return c == null ? "none" : c.name;
        }

        private static Transform FindEmptyRow(Transform panel)
        {
            foreach (Transform t in panel.GetComponentsInChildren<Transform>(true))
            {
                if (t.name == "Empty" && t.GetComponent(Ui.T17Text) != null && t.GetComponent(Ui.T17Button) == null)
                {
                    return t;
                }
            }
            return null;
        }

        private static Sprite FindTallTicketSprite(Transform panel)
        {
            foreach (Transform t in panel.GetComponentsInChildren<Transform>(true))
            {
                Sprite s = Ui.GetSprite(Ui.Image(t.gameObject));
                if (s != null && s.name.IndexOf("bodyExtended", StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    return s;
                }
            }
            return null;
        }

        private static void ExtendMenuBodies(Component panel, int index, Component body)
        {
            Array current = Ui.Field(panel, "m_MenuBodies") as Array;
            if (current == null || current.Length > index)
            {
                return;
            }
            Array longer = Array.CreateInstance(current.GetType().GetElementType(), index + 1);
            Array.Copy(current, longer, current.Length);
            longer.SetValue(body, index);
            Ui.SetField(panel, "m_MenuBodies", longer);
        }

        private static void FitTabs(List<Component> tabs, List<Component> bodies)
        {
            int n = tabs.Count;
            RectTransform first = (RectTransform)tabs[0].transform;
            RectTransform last = (RectTransform)tabs[n - 2].transform;
            float spacing = (last.anchoredPosition.x - first.anchoredPosition.x) / (n - 2);
            float left = first.anchoredPosition.x - first.rect.width * 0.5f;
            float width = last.anchoredPosition.x + last.rect.width * 0.5f - left;
            float scale = width / (width + spacing);
            float lastX = last.anchoredPosition.x;
            for (int i = 0; i < n; i++)
            {
                float rawX = ((RectTransform)tabs[i].transform).anchoredPosition.x;
                float x = left + first.rect.width * scale * 0.5f + i * spacing * scale;
                LayoutOffset.Attach(tabs[i].gameObject, new Vector2(x - rawX, 0f), scale);
                if (i < n - 1)
                {
                    LayoutOffset.Attach(bodies[i].gameObject, new Vector2(x - rawX, 0f), 1f);
                }
                else
                {
                    Slot(bodies[i], x - lastX);
                }
            }
        }

        private static void Slot(Component body, float dx)
        {
            RectTransform parent = (RectTransform)body.transform.parent;
            GameObject slot = new GameObject("SwoftyModsSlot", typeof(RectTransform));
            RectTransform rt = (RectTransform)slot.transform;
            rt.SetParent(parent, false);
            rt.anchorMin = Vector2.zero;
            rt.anchorMax = Vector2.one;
            rt.pivot = parent.pivot;
            rt.sizeDelta = Vector2.zero;
            rt.anchoredPosition = new Vector2(dx, 0f);
            rt.SetSiblingIndex(body.transform.GetSiblingIndex());
            body.transform.SetParent(rt, false);
        }

        private static void Sync(LiveMenu m)
        {
            List<MenuButton> wanted = MainMenu.Snapshot().FindAll(b => b.Visible);
            List<string> keep = new List<string>();
            foreach (MenuButton b in wanted)
            {
                keep.Add(b.Id);
            }
            foreach (string id in new List<string>(m.Entries.Keys))
            {
                if (!keep.Contains(id))
                {
                    UnityEngine.Object.Destroy(m.Entries[id]);
                    m.Entries.Remove(id);
                }
            }
            List<Component> order = new List<Component>();
            int sibling = 0;
            foreach (MenuButton b in wanted)
            {
                GameObject entry;
                if (!m.Entries.TryGetValue(b.Id, out entry) || entry == null)
                {
                    entry = UnityEngine.Object.Instantiate(m.EntryTemplate, m.Body.transform, false);
                    entry.name = "Swofty_" + b.Id;
                    MenuButton target = b;
                    Ui.ReplaceClick(entry.GetComponent(Ui.T17Button), () => OnEntryClicked(target));
                    m.Entries[b.Id] = entry;
                }
                Ui.SetLabel(entry, (b.Label ?? string.Empty).ToUpperInvariant());
                entry.transform.SetSiblingIndex(sibling++);
                entry.SetActive(true);
                order.Add(entry.GetComponent(Ui.T17Button));
            }
            foreach (GameObject spacer in m.Spacers)
            {
                UnityEngine.Object.DestroyImmediate(spacer);
            }
            m.Spacers.Clear();
            if (m.SpacerTemplate != null)
            {
                for (int i = wanted.Count; i < TicketRows; i++)
                {
                    GameObject spacer = UnityEngine.Object.Instantiate(m.SpacerTemplate, m.Body.transform, false);
                    spacer.name = "Empty";
                    spacer.transform.SetSiblingIndex(sibling++);
                    spacer.SetActive(true);
                    m.Spacers.Add(spacer);
                }
            }
            if (m.TicketNumber != null)
            {
                m.TicketNumber.transform.SetAsLastSibling();
            }
            ResizeTicket(m, Math.Max(TicketRows, wanted.Count));
            for (int i = 0; i < order.Count; i++)
            {
                Ui.SetNavigation(order[i], "selectOnUp", i == 0 ? m.TabButton : order[i - 1]);
                Ui.SetNavigation(order[i], "selectOnDown", i == order.Count - 1 ? null : order[i + 1]);
                Ui.SetNavigation(order[i], "selectOnLeft", m.PreviousTab);
                Ui.SetNavigation(order[i], "selectOnRight", null);
            }
            if (order.Count > 0)
            {
                Ui.SetBorderTop(m.BodyMenu, order[0]);
            }
        }

        private static void ResizeTicket(LiveMenu m, int rows)
        {
            RectTransform rt = (RectTransform)m.Body.transform;
            Component image = Ui.Image(m.Body);
            if (rows <= TicketRows || m.TallBodySprite == null)
            {
                Ui.SetSprite(image, m.BodySprite);
                rt.sizeDelta = new Vector2(rt.sizeDelta.x, m.BodyHeight + Math.Max(0, rows - TicketRows) * 43f);
                return;
            }
            Ui.SetSprite(image, m.TallBodySprite);
            rt.sizeDelta = new Vector2(rt.sizeDelta.x, m.BodyHeight + (rows - TicketRows) * 59f);
        }

        private static void OnEntryClicked(MenuButton button)
        {
            host.Log.Info("main menu button " + button.Id + " clicked");
            button.Click();
        }

        private sealed class LiveMenu
        {
            internal GameObject Tab;
            internal Component TabButton;
            internal Component PreviousTab;
            internal GameObject Body;
            internal Component BodyMenu;
            internal GameObject EntryTemplate;
            internal GameObject SpacerTemplate;
            internal GameObject TicketNumber;
            internal Sprite BodySprite;
            internal Sprite TallBodySprite;
            internal float BodyHeight;
            internal readonly Dictionary<string, GameObject> Entries = new Dictionary<string, GameObject>();
            internal readonly List<GameObject> Spacers = new List<GameObject>();

            internal bool Alive
            {
                get { return Tab != null && Body != null; }
            }
        }
    }
}
