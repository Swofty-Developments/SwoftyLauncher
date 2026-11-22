use gpui_kit::assets::IconName;
use swofty_i18n::{tr, trf};

use super::Page;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Library,
    Discover,
    Online,
    Store,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Home, Tab::Library, Tab::Discover, Tab::Online, Tab::Store];

    pub fn label(self) -> &'static str {
        match self {
            Self::Home => tr("Home"),
            Self::Library => tr("Library"),
            Self::Discover => tr("Discover"),
            Self::Online => tr("Online"),
            Self::Store => tr("Donate"),
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Home => IconName::Newspaper,
            Self::Library => IconName::Puzzle,
            Self::Discover => IconName::PackageSearch,
            Self::Online => IconName::Trophy,
            Self::Store => IconName::Heart,
        }
    }

    pub fn tint(self, p: &crate::ui::theme::Palette) -> gpui_kit::Hsla {
        match self {
            Self::Home => p.tab_home,
            Self::Library => p.tab_library,
            Self::Discover => p.tab_discover,
            Self::Online => p.tab_trial,
            Self::Store => p.tab_store,
        }
    }

    pub fn home(self) -> Page {
        match self {
            Self::Home => Page::News,
            Self::Library => Page::Addons,
            Self::Discover => Page::Browser,
            Self::Online => Page::Online,
            Self::Store => Page::Store,
        }
    }
}

pub const ACTIONS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Notifications,
    Settings,
    Account,
}

impl Action {
    pub const ALL: [Action; ACTIONS] = [Action::Notifications, Action::Settings, Action::Account];
}

impl Page {
    pub fn tab(self) -> Option<Tab> {
        match self {
            Self::News => Some(Tab::Home),
            Self::Browser => Some(Tab::Discover),
            Self::Online | Self::Multiplayer | Self::Moderation => Some(Tab::Online),
            Self::Store => Some(Tab::Store),
            Self::Settings | Self::Tools => None,
            Self::Addons
            | Self::Shaders
            | Self::Downloads
            | Self::Importer
            | Self::Careers
            | Self::Database
            | Self::Section(_) => Some(Tab::Library),
        }
    }

    pub fn sub_label(self) -> &'static str {
        match self {
            Self::Addons => tr("Addons"),
            Self::Online => tr("Time Trial"),
            _ => self.nav_label(),
        }
    }
}

/// The two pages of the Online tab with a trail bar: Time Trial, and Multiplayer for a player with access.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OnlineTab {
    Multiplayer,
    TimeTrial,
}

/// The pages under the Online tab: Time Trial first, as in 1.10, then Multiplayer with access and Moderation for
/// a Time Trial admin.
pub fn online_pages(access: bool, admin: bool) -> Vec<Page> {
    let mut pages = vec![Page::Online];
    if access {
        pages.push(Page::Multiplayer);
    }
    if admin {
        pages.push(Page::Moderation);
    }
    pages
}

/// Where the Online tab stands once access to the rooms is given or taken away: off the Multiplayer page without
/// it.
pub fn after_access(access: bool, page: Page) -> Page {
    match page {
        Page::Multiplayer if !access => Page::Online,
        page => page,
    }
}

/// The Multiplayer page's name with what is going on there: the player's room, or how many rooms are open once
/// the list has loaded.
pub fn rooms_label(in_room: bool, open: Option<usize>) -> String {
    match (in_room, open) {
        (true, _) => tr("Multiplayer \u{b7} in a room").to_owned(),
        (false, Some(count)) => trf!("Multiplayer \u{b7} {count} open", count = count),
        (false, None) => tr("Multiplayer").to_owned(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Tabs,
    Links,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Stay,
    IntoPage,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cursor {
    pub row: Row,
    pub col: usize,
}

impl Cursor {
    pub fn on_tab(tab: Option<Tab>) -> Self {
        let col = tab
            .and_then(|t| Tab::ALL.iter().position(|x| *x == t))
            .unwrap_or(0);
        Self { row: Row::Tabs, col }
    }

    pub fn apply(&mut self, key: Key, tabs: usize, links: usize) -> Step {
        let len = match self.row {
            Row::Tabs => tabs,
            Row::Links => links,
        };
        match key {
            Key::Left => self.col = self.col.saturating_sub(1),
            Key::Right => self.col = (self.col + 1).min(len.saturating_sub(1)),
            Key::Up => {
                if self.row == Row::Links {
                    self.row = Row::Tabs;
                    self.col = self.col.min(tabs.saturating_sub(1));
                }
            }
            Key::Down => match self.row {
                Row::Tabs if links > 0 => {
                    self.row = Row::Links;
                    self.col = 0;
                }
                _ => return Step::IntoPage,
            },
        }
        Step::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP: usize = Tab::ALL.len() + ACTIONS;

    #[test]
    fn every_page_has_a_tab_or_the_cog() {
        assert_eq!(Page::Browser.tab(), Some(Tab::Discover));
        assert_eq!(Page::Shaders.tab(), Some(Tab::Library));
        assert_eq!(Page::Section(3).tab(), Some(Tab::Library));
        assert_eq!(Page::Multiplayer.tab(), Some(Tab::Online));
        assert_eq!(Page::Moderation.tab(), Some(Tab::Online));
        assert_eq!(Page::Settings.tab(), None);
        assert_eq!(Page::Tools.tab(), None);
        for tab in Tab::ALL {
            assert_eq!(tab.home().tab(), Some(tab));
        }
    }

    #[test]
    fn the_tabs_never_name_the_gated_page() {
        let labels: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(labels, ["Home", "Library", "Discover", "Online", "Donate"]);
        assert!(labels.iter().all(|l| !l.to_lowercase().contains("multiplayer")));
    }

    #[test]
    fn online_opens_time_trial_with_access_or_without() {
        assert!(Tab::Online.home() == Page::Online);
    }

    #[test]
    fn time_trial_stays_the_first_online_page_and_multiplayer_follows_it_with_access() {
        assert!(online_pages(false, false) == vec![Page::Online]);
        assert!(online_pages(false, true) == vec![Page::Online, Page::Moderation]);
        assert!(online_pages(true, false) == vec![Page::Online, Page::Multiplayer]);
        assert!(online_pages(true, true) == vec![Page::Online, Page::Multiplayer, Page::Moderation]);
    }

    #[test]
    fn losing_access_leaves_the_multiplayer_page() {
        assert!(after_access(false, Page::Multiplayer) == Page::Online);
        assert!(after_access(false, Page::Online) == Page::Online);
        assert!(after_access(false, Page::News) == Page::News);
        assert!(after_access(false, Page::Moderation) == Page::Moderation);
        assert!(after_access(true, Page::Multiplayer) == Page::Multiplayer);
        assert!(after_access(true, Page::Online) == Page::Online);
    }

    #[test]
    fn the_multiplayer_page_names_what_is_going_on_there() {
        assert_eq!(rooms_label(false, None), "Multiplayer");
        assert_eq!(rooms_label(false, Some(3)), "Multiplayer \u{b7} 3 open");
        assert_eq!(rooms_label(true, Some(3)), "Multiplayer \u{b7} in a room");
    }

    #[test]
    fn left_and_right_stop_at_the_ends() {
        let mut c = Cursor::on_tab(Some(Tab::Home));
        assert_eq!(c.apply(Key::Left, TOP, 0), Step::Stay);
        assert_eq!(c.col, 0);
        for _ in 0..12 {
            c.apply(Key::Right, TOP, 0);
        }
        assert_eq!(c.col, TOP - 1);
    }

    #[test]
    fn the_bar_reaches_the_actions_after_the_tabs() {
        let mut c = Cursor::on_tab(Some(Tab::Store));
        c.apply(Key::Right, TOP, 0);
        assert_eq!(c.col, Tab::ALL.len());
        assert_eq!(Action::ALL[c.col - Tab::ALL.len()], Action::Notifications);
    }

    #[test]
    fn down_goes_to_the_links_then_the_page() {
        let mut c = Cursor::on_tab(Some(Tab::Library));
        assert_eq!(c.apply(Key::Down, TOP, 3), Step::Stay);
        assert_eq!((c.row, c.col), (Row::Links, 0));
        c.apply(Key::Right, TOP, 3);
        assert_eq!(c.apply(Key::Up, TOP, 3), Step::Stay);
        assert_eq!(c.row, Row::Tabs);
        c.apply(Key::Down, TOP, 3);
        assert_eq!(c.apply(Key::Down, TOP, 3), Step::IntoPage);
    }

    #[test]
    fn down_skips_an_empty_link_row() {
        let mut c = Cursor::on_tab(Some(Tab::Home));
        assert_eq!(c.apply(Key::Down, TOP, 0), Step::IntoPage);
    }
}
