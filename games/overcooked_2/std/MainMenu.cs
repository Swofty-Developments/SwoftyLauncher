using System;
using System.Collections.Generic;

namespace Swofty.Overcooked2
{
    /// <summary>The front-end main menu. Swofty Core adds a MODS tab after the game's own tabs; its ticket lists the buttons.</summary>
    public static class MainMenu
    {
        private static readonly List<MenuButton> buttons = new List<MenuButton>();

        /// <summary>The main menu has shown.</summary>
        public static event Action Shown;

        /// <summary>
        /// Adds a button to the MODS tab of the main menu, in the game's own ticket style. Buttons appear in the order
        /// added. The label is shown in capitals like the game's own entries and is not looked up as a localisation key.
        /// </summary>
        public static MenuButton AddButton(string id, string label, Action onClick)
        {
            if (string.IsNullOrEmpty(id))
            {
                throw new ArgumentException("a menu button needs an id", "id");
            }
            if (onClick == null)
            {
                throw new ArgumentNullException("onClick");
            }
            lock (buttons)
            {
                foreach (MenuButton existing in buttons)
                {
                    if (existing.Id == id)
                    {
                        throw new InvalidOperationException("a main menu button with id " + id + " exists already");
                    }
                }
                MenuButton button = new MenuButton(id, label ?? id, onClick, AddonScope.Current);
                buttons.Add(button);
                Notify(button, MenuChange.Added);
                return button;
            }
        }

        internal static IMainMenuBackend Backend { get; set; }

        internal static List<MenuButton> Snapshot()
        {
            lock (buttons)
            {
                return new List<MenuButton>(buttons);
            }
        }

        internal static void RemoveOwnedBy(string owner)
        {
            List<MenuButton> removed = new List<MenuButton>();
            lock (buttons)
            {
                buttons.RemoveAll(b =>
                {
                    if (b.Owner != owner)
                    {
                        return false;
                    }
                    removed.Add(b);
                    return true;
                });
            }
            foreach (MenuButton b in removed)
            {
                Notify(b, MenuChange.Removed);
            }
        }

        internal static void RemoveHandlersOwnedBy(string owner)
        {
            Shown = Events.Without(Shown, owner);
        }

        internal static void Changed(MenuButton button)
        {
            Notify(button, MenuChange.Changed);
        }

        internal static void RaiseShown()
        {
            Events.Raise(Shown, "MainMenu.Shown");
        }

        internal static void ResetForTests()
        {
            lock (buttons)
            {
                buttons.Clear();
            }
            Shown = null;
            Backend = null;
        }

        private static void Notify(MenuButton button, MenuChange change)
        {
            IMainMenuBackend backend = Backend;
            if (backend != null)
            {
                backend.OnButtonChanged(button, change);
            }
        }
    }

    /// <summary>A button an addon added to the main menu.</summary>
    public sealed class MenuButton
    {
        private string label;
        private bool visible = true;

        internal MenuButton(string id, string label, Action onClick, string owner)
        {
            Id = id;
            this.label = label;
            OnClick = onClick;
            Owner = owner;
        }

        public string Id { get; private set; }

        /// <summary>The text on the button; the MODS ticket shows it in capitals.</summary>
        public string Label
        {
            get { return label; }
            set
            {
                label = value ?? string.Empty;
                MainMenu.Changed(this);
            }
        }

        /// <summary>Whether the button is listed on the MODS tab.</summary>
        public bool Visible
        {
            get { return visible; }
            set
            {
                if (visible == value)
                {
                    return;
                }
                visible = value;
                MainMenu.Changed(this);
            }
        }

        internal Action OnClick { get; private set; }

        internal string Owner { get; private set; }

        internal void Click()
        {
            Events.Invoke(OnClick, "main menu button " + Id);
        }
    }

    internal enum MenuChange
    {
        Added,
        Changed,
        Removed,
    }

    internal interface IMainMenuBackend
    {
        void OnButtonChanged(MenuButton button, MenuChange change);
    }
}
