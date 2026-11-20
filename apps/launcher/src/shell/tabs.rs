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

/// The two pages of the Online tab for a player with access. The one used last is kept between starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OnlineTab {
    Multiplayer,
    TimeTrial,
}

impl OnlineTab {
    pub fn of(page: Page) -> Option<Self> {
        match page {
            Page::Multiplayer => Some(Self::Multiplayer),
            Page::Online => Some(Self::TimeTrial),
            _ => None,
        }
    }

    pub fn page(self) -> Page {
        match self {
            Self::Multiplayer => Page::Multiplayer,
            Self::TimeTrial => Page::Online,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Multiplayer => "multiplayer",
            Self::TimeTrial => "time-trial",
        }
    }

    fn path(data: &std::path::Path) -> std::path::PathBuf {
        data.join("state").join("online-tab.txt")
    }

    pub fn load(data: &std::path::Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(data)).ok()?;
        [Self::Multiplayer, Self::TimeTrial]
            .into_iter()
            .find(|t| t.name() == text.trim())
    }

    pub fn save(self, data: &std::path::Path) {
        let path = Self::path(data);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(path, self.name()).ok();
    }

    /// Keeps `tab` as the one used last, or forgets the one kept.
    pub fn keep(tab: Option<Self>, data: &std::path::Path) {
        match tab {
            Some(tab) => tab.save(data),
            None => {
                std::fs::remove_file(Self::path(data)).ok();
            }
        }
    }
}

/// The page the Online tab opens: Time Trial without access, the room's page for a player in a room, else the
/// tab used last, and Multiplayer the first time.
pub fn online_home(access: bool, in_room: bool, last: Option<OnlineTab>) -> Page {
    if !access {
        return Page::Online;
    }
    if in_room {
        return Page::Multiplayer;
    }
    last.unwrap_or(OnlineTab::Multiplayer).page()
}

/// The pages under the Online tab: Time Trial alone without access, as in 1.10, and Multiplayer before it with
/// access. Moderation follows for a Time Trial admin.
pub fn online_pages(access: bool, admin: bool) -> Vec<Page> {
    let mut pages = Vec::new();
    if access {
        pages.push(Page::Multiplayer);
    }
    pages.push(Page::Online);
    if admin {
        pages.push(Page::Moderation);
    }
    pages
}

/// Where the Online tab stands once access to the rooms is given or taken away: off the Multiplayer page and
/// with no Multiplayer tab kept as the one used last, without access.
pub fn after_access(access: bool, page: Page, kept: Option<OnlineTab>) -> (Page, Option<OnlineTab>) {
    if access {
        return (page, kept);
    }
    let page = match page {
        Page::Multiplayer => Page::Online,
        page => page,
    };
    (page, kept.filter(|t| *t != OnlineTab::Multiplayer))
}

/// The Multiplayer tab's name with what is going on there: the player's room, or how many rooms are open once
/// the list has loaded.
pub fn rooms_label(in_room: bool, open: Option<usize>) -> String {
    match (in_room, open) {
        (true, _) => tr("Multiplayer \u{b7} in a room").to_owned(),
        (false, Some(count)) => trf!("Multiplayer \u{b7} {count} open", count = count),
        (false, None) => tr("Multiplayer").to_owned(),
    }
}

/// The Time Trial tab's name with the player's place in the global ranking, once it is known.
pub fn trial_label(rank: Option<u32>) -> String {
    match rank {
        Some(rank) => trf!("Time Trial \u{b7} #{rank}", rank = rank),
        None => tr("Time Trial").to_owned(),
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
    fn online_opens_time_trial_without_access_the_room_in_one_else_the_tab_used_last() {
        assert!(online_home(false, false, Some(OnlineTab::Multiplayer)) == Page::Online);
        assert!(online_home(false, true, None) == Page::Online);
        assert!(online_home(true, false, None) == Page::Multiplayer);
        assert!(online_home(true, false, Some(OnlineTab::TimeTrial)) == Page::Online);
        assert!(online_home(true, true, Some(OnlineTab::TimeTrial)) == Page::Multiplayer);
    }

    #[test]
    fn without_access_online_has_time_trial_alone_as_in_1_10() {
        assert!(online_pages(false, false) == vec![Page::Online]);
        assert!(online_pages(false, true) == vec![Page::Online, Page::Moderation]);
        assert!(online_pages(true, false) == vec![Page::Multiplayer, Page::Online]);
        assert!(online_pages(true, true) == vec![Page::Multiplayer, Page::Online, Page::Moderation]);
    }

    #[test]
    fn losing_access_leaves_the_multiplayer_page_and_forgets_it_as_the_tab_used_last() {
        let mp = Some(OnlineTab::Multiplayer);
        let tt = Some(OnlineTab::TimeTrial);
        assert!(after_access(false, Page::Multiplayer, mp) == (Page::Online, None));
        assert!(after_access(false, Page::Online, mp) == (Page::Online, None));
        assert!(after_access(false, Page::News, tt) == (Page::News, tt));
        assert!(after_access(false, Page::Moderation, None) == (Page::Moderation, None));
        assert!(after_access(true, Page::Multiplayer, mp) == (Page::Multiplayer, mp));
        assert!(after_access(true, Page::Online, tt) == (Page::Online, tt));
        let dir = std::env::temp_dir().join(format!("swofty-online-keep-{}", std::process::id()));
        OnlineTab::keep(mp, &dir);
        assert_eq!(OnlineTab::load(&dir), mp);
        OnlineTab::keep(None, &dir);
        assert_eq!(OnlineTab::load(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_online_tab_used_last_is_kept_between_starts() {
        let dir = std::env::temp_dir().join(format!("swofty-online-tab-{}", std::process::id()));
        assert_eq!(OnlineTab::load(&dir), None);
        OnlineTab::TimeTrial.save(&dir);
        assert_eq!(OnlineTab::load(&dir), Some(OnlineTab::TimeTrial));
        OnlineTab::Multiplayer.save(&dir);
        assert_eq!(OnlineTab::load(&dir), Some(OnlineTab::Multiplayer));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(OnlineTab::of(Page::Online), Some(OnlineTab::TimeTrial));
        assert_eq!(OnlineTab::of(Page::Moderation), None);
    }

    #[test]
    fn the_online_pages_name_what_is_going_on_in_each() {
        assert_eq!(rooms_label(false, None), "Multiplayer");
        assert_eq!(rooms_label(false, Some(3)), "Multiplayer \u{b7} 3 open");
        assert_eq!(rooms_label(true, Some(3)), "Multiplayer \u{b7} in a room");
        assert_eq!(trial_label(None), "Time Trial");
        assert_eq!(trial_label(Some(138)), "Time Trial \u{b7} #138");
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
