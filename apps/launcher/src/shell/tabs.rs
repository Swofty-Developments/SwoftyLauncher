use gpui_kit::assets::IconName;
use swofty_i18n::tr;

use super::Page;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    News,
    Addons,
    Shaders,
    Online,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::News, Tab::Addons, Tab::Shaders, Tab::Online, Tab::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Self::News => tr("News"),
            Self::Addons => tr("Addons"),
            Self::Shaders => tr("Shaders"),
            Self::Online => tr("Online"),
            Self::Settings => tr("Settings"),
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::News => IconName::Newspaper,
            Self::Addons => IconName::Puzzle,
            Self::Shaders => IconName::Palette,
            Self::Online => IconName::Trophy,
            Self::Settings => IconName::Settings,
        }
    }

    pub fn home(self) -> Page {
        match self {
            Self::News => Page::News,
            Self::Addons => Page::Addons,
            Self::Shaders => Page::Shaders,
            Self::Online => Page::Online,
            Self::Settings => Page::Settings,
        }
    }
}

impl Page {
    pub fn tab(self) -> Tab {
        match self {
            Self::News => Tab::News,
            Self::Shaders => Tab::Shaders,
            Self::Online | Self::Multiplayer => Tab::Online,
            Self::Settings | Self::Tools => Tab::Settings,
            Self::Addons
            | Self::Browser
            | Self::Downloads
            | Self::Importer
            | Self::Profiles
            | Self::Careers
            | Self::Database
            | Self::Section(_) => Tab::Addons,
        }
    }

    pub fn sub_label(self) -> &'static str {
        match self {
            Self::Addons => tr("Installed"),
            Self::Browser => tr("Browser"),
            Self::Online => tr("Time Trial"),
            _ => self.nav_label(),
        }
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
    pub fn on_tab(tab: Tab) -> Self {
        let col = Tab::ALL.iter().position(|t| *t == tab).unwrap_or(0);
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

    #[test]
    fn every_page_has_a_tab() {
        assert_eq!(Page::Browser.tab(), Tab::Addons);
        assert_eq!(Page::Section(3).tab(), Tab::Addons);
        assert_eq!(Page::Multiplayer.tab(), Tab::Online);
        assert_eq!(Page::Tools.tab(), Tab::Settings);
        for tab in Tab::ALL {
            assert_eq!(tab.home().tab(), tab);
        }
    }

    #[test]
    fn left_and_right_stop_at_the_ends() {
        let mut c = Cursor::on_tab(Tab::News);
        assert_eq!(c.apply(Key::Left, 5, 0), Step::Stay);
        assert_eq!(c.col, 0);
        for _ in 0..9 {
            c.apply(Key::Right, 5, 0);
        }
        assert_eq!(c.col, 4);
    }

    #[test]
    fn down_goes_to_the_links_then_the_page() {
        let mut c = Cursor::on_tab(Tab::Addons);
        assert_eq!(c.apply(Key::Down, 5, 3), Step::Stay);
        assert_eq!((c.row, c.col), (Row::Links, 0));
        c.apply(Key::Right, 5, 3);
        assert_eq!(c.apply(Key::Up, 5, 3), Step::Stay);
        assert_eq!(c.row, Row::Tabs);
        c.apply(Key::Down, 5, 3);
        assert_eq!(c.apply(Key::Down, 5, 3), Step::IntoPage);
    }

    #[test]
    fn down_skips_an_empty_link_row() {
        let mut c = Cursor::on_tab(Tab::News);
        assert_eq!(c.apply(Key::Down, 5, 0), Step::IntoPage);
    }
}
