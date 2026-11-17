mod bar;
mod popovers;

use gpui_kit::assets::IconName;
use gpui_kit::*;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use swofty_i18n::trn;

use crate::shell::overflow::{Link, split_overflow};
use crate::shell::tabs::{ACTIONS, Action, Cursor, Key, Row, Step, Tab};
use crate::shell::{Page, Shell};

pub(crate) const BAR_HEIGHT: f32 = 68.;
pub(crate) const SUB_HEIGHT: f32 = 40.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Popover {
    Notices,
    Account,
}

pub(crate) struct Notice {
    pub icon: IconName,
    pub text: String,
    pub page: Option<Page>,
}

pub(crate) struct SubNav {
    pub split: Option<usize>,
    pub inline: Vec<Link>,
    pub more: Vec<Link>,
}

impl SubNav {
    fn flat(&self) -> Vec<Link> {
        self.inline.iter().chain(&self.more).cloned().collect()
    }
}

fn filled_icon(icon: IconName, cx: &App) -> Option<Arc<[u8]>> {
    type Cache = Mutex<HashMap<String, Option<Arc<[u8]>>>>;
    static CACHE: LazyLock<Cache> = LazyLock::new(Mutex::default);
    let path = icon.path();
    let mut cache = CACHE.lock().ok()?;
    cache
        .entry(path.to_string())
        .or_insert_with(|| {
            let bytes = cx.asset_source().load(&path).ok().flatten()?;
            let text = String::from_utf8_lossy(&bytes)
                .replace("fill=\"none\"", "fill=\"currentColor\" fill-opacity=\"0.45\"");
            Some(Arc::from(text.into_bytes()))
        })
        .clone()
}

impl Shell {
    pub(crate) fn nav_mp(&self) -> bool {
        self.mp_allowed() || (cfg!(debug_assertions) && std::env::var_os("SWOFTY_FAKE_MP").is_some())
    }

    pub(crate) fn account_name(&self) -> Option<String> {
        self.mp.as_ref().map(|s| s.name.clone()).or_else(|| {
            cfg!(debug_assertions)
                .then(|| std::env::var("SWOFTY_FAKE_SIGNED_IN").ok())
                .flatten()
        })
    }

    pub(crate) fn sub_nav(&self, layout_width: f32) -> Option<SubNav> {
        let tab = self.page.tab();
        let (links, fixed) = match tab {
            Some(Tab::Library) => {
                let mut links = vec![
                    Link::new(Page::Addons, Some(self.library.listed().count())),
                    Link::new(Page::Shaders, None),
                    Link::new(Page::Careers, None),
                ];
                links.extend(self.addon_page_links());
                (links, 3)
            }
            Some(Tab::Online) => {
                let mut links = vec![Link::new(Page::Online, None)];
                if self.nav_mp() {
                    links.push(Link::new(Page::Multiplayer, None));
                }
                if self.tt_admin() {
                    links.push(Link::new(Page::Moderation, None));
                }
                let n = links.len();
                (links, n)
            }
            None if self.developer_mode => (
                vec![Link::new(Page::Settings, None), Link::new(Page::Tools, None)],
                2,
            ),
            _ => return None,
        };
        let (inline, more) = split_overflow(links, fixed, (layout_width - 140.).max(300.));
        let split = (tab == Some(Tab::Library) && inline.len() + more.len() > fixed).then_some(fixed);
        Some(SubNav { split, inline, more })
    }

    fn addon_page_links(&self) -> Vec<Link> {
        let mut links = Vec::new();
        let mut importer = false;
        for (i, s) in self.library.sections.iter().enumerate() {
            if !s.is_simplified() || s.is_importer_tracks() {
                continue;
            }
            if s.is_importer() {
                importer = true;
                links.push(Link::new(Page::Importer, None));
                continue;
            }
            links.push(Link {
                page: Page::Section(i),
                label: s.spec.title.clone().into(),
                count: None,
            });
        }
        if !importer {
            links.push(Link::new(Page::Importer, None));
        }
        links.push(Link::new(
            Page::Database,
            Some(self.db_count()).filter(|n| *n > 0),
        ));
        for (i, s) in self.library.sections.iter().enumerate() {
            if s.is_simplified() {
                continue;
            }
            links.push(Link {
                page: Page::Section(i),
                label: s.spec.title.clone().into(),
                count: None,
            });
        }
        links
    }

    pub(crate) fn notices(&self) -> Vec<Notice> {
        let mut out: Vec<Notice> = self
            .launcher_notices()
            .into_iter()
            .map(|text| Notice {
                icon: IconName::CircleArrowUp,
                text,
                page: None,
            })
            .collect();
        let n = self.addon_updates().len();
        if n > 0 {
            out.push(Notice {
                icon: IconName::Download,
                text: trn(
                    n,
                    "{count} addon update available",
                    "{count} addon updates available",
                ),
                page: Some(Page::Browser),
            });
        }
        out
    }

    fn open_popover(&mut self, which: Popover, cx: &mut Context<Self>) {
        self.popover = (self.popover != Some(which)).then_some(which);
        if self.popover == Some(Popover::Notices) {
            cx.update_global(|n: &mut crate::shell::notices::Notices, _| {
                n.mark_read();
                n.dismiss_popups();
            });
        }
        cx.notify();
    }

    pub(crate) fn nav_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        let plain = !ev.keystroke.modifiers.control && !ev.keystroke.modifiers.platform;
        if key == "escape" && self.page == Page::Addons && self.addons.clear_selection() {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "escape" && self.popover.is_some() {
            self.popover = None;
            cx.notify();
            return;
        }
        if self.nav.is_none()
            && self.page == Page::Online
            && self.root_focus.is_focused(window)
            && plain
            && (matches!(key, "down" | "right" | "enter" | "escape" | "backspace")
                || (matches!(key, "up" | "left") && self.online_has_cursor()))
            && self.online_key(key, cx)
        {
            cx.stop_propagation();
            return;
        }
        let Some(mut cursor) = self.nav else {
            let wanted = key == "up"
                && plain
                && (self.ui_mode.is_some_and(|m| m.is_tv()) || ev.keystroke.modifiers.alt);
            if wanted && self.root_focus.is_focused(window) {
                self.nav = Some(Cursor::on_tab(self.page.tab()));
                cx.stop_propagation();
                cx.notify();
            }
            return;
        };
        let links = self
            .sub_nav(self.layout_width)
            .map(|s| s.flat())
            .unwrap_or_default();
        let moved = match key {
            "left" => Some(Key::Left),
            "right" => Some(Key::Right),
            "up" => Some(Key::Up),
            "down" => Some(Key::Down),
            _ => None,
        };
        cx.stop_propagation();
        if let Some(k) = moved {
            if cursor.apply(k, Tab::ALL.len() + ACTIONS, links.len()) == Step::IntoPage {
                self.nav = None;
                self.popover = None;
                window.focus(&self.root_focus, cx);
            } else {
                self.nav = Some(cursor);
            }
        } else if key == "enter" || key == "space" {
            match cursor.row {
                Row::Tabs if cursor.col < Tab::ALL.len() => {
                    let tab = Tab::ALL[cursor.col];
                    let page = if self.page.tab() == Some(tab) {
                        self.page
                    } else {
                        tab.home()
                    };
                    self.open_page(page, cx);
                }
                Row::Tabs => match Action::ALL[cursor.col - Tab::ALL.len()] {
                    Action::Notifications => self.open_popover(Popover::Notices, cx),
                    Action::Settings => self.open_page(Page::Settings, cx),
                    Action::Account => {
                        if self.account_name().is_some() {
                            self.open_popover(Popover::Account, cx);
                        } else {
                            self.mp_sign_in(window, cx);
                        }
                    }
                },
                Row::Links => {
                    if let Some(l) = links.get(cursor.col) {
                        self.open_page(l.page, cx);
                    }
                }
            }
        } else if key == "escape" {
            self.nav = None;
            window.focus(&self.root_focus, cx);
        }
        cx.notify();
    }
}
