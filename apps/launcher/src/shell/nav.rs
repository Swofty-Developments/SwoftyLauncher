use gpui_kit::component::button::Button;
use gpui_kit::*;
use std::time::Instant;

use super::tabs::{OnlineTab, Tab, after_access, online_home};
use super::{Page, Shell};

/// Which page is open, where keyboard focus is in the tabs, and what the top bar has open.
pub(crate) struct Navigation {
    pub(crate) page: Page,
    pub(crate) cursor: Option<super::tabs::Cursor>,
    pub(crate) popover: Option<super::topbar::Popover>,
    pub(crate) profiles_open: bool,
    pub(crate) open_addon: Option<String>,
    /// The addon the Addons page highlights, and since when.
    pub(crate) highlight: Option<(String, Instant)>,
    /// The Online page a player with access used last.
    pub(crate) online_tab: Option<OnlineTab>,
}

impl Shell {
    /// Goes back from the addon page that's open: along the Addon Browser's trail, or out of
    /// the Addons page's panel. Back, Escape, Alt+Left and the mouse's back button all come here.
    pub(crate) fn page_back(&mut self, cx: &mut Context<Self>) -> bool {
        match self.navigation.page {
            Page::Browser if self.browser.has_back() => self.browse_back(cx),
            Page::Addons if self.navigation.open_addon.is_some() => self.close_addon_panel(cx),
            _ => return false,
        }
        true
    }

    pub(crate) fn page_back_button(&self, cx: &mut Context<Self>) -> Button {
        crate::ui::back_button("page-back").on_click(cx.listener(|this, _, _, cx| {
            this.page_back(cx);
        }))
    }

    pub(crate) fn cancel_scroll(&self) {
        self.news_scroll.cancel();
        self.addons.cancel_scroll();
        self.browser.smooth.cancel();
        self.db.rows_motion.cancel();
        self.db.list_motion.cancel();
    }

    /// The page a tab opens when another tab is open.
    pub(crate) fn tab_home(&self, tab: Tab) -> Page {
        match tab {
            Tab::Online => online_home(self.nav_mp(), self.rooms.in_room(), self.navigation.online_tab),
            _ => tab.home(),
        }
    }

    /// Follows access to the rooms being given or taken away: off the Multiplayer page without it, and no
    /// Multiplayer tab kept as the one used last.
    pub(crate) fn follow_online_access(&mut self, cx: &mut Context<Self>) {
        let (page, kept) = after_access(self.nav_mp(), self.navigation.page, self.navigation.online_tab);
        if kept != self.navigation.online_tab {
            self.navigation.online_tab = kept;
            OnlineTab::keep(kept, &self.store.root);
        }
        if page != self.navigation.page {
            self.open_page(page, cx);
        }
    }

    pub(crate) fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        let page = match page {
            Page::Multiplayer if !self.nav_mp() => Page::Online,
            page => page,
        };
        if self.navigation.page != page {
            self.cancel_scroll();
        }
        self.db_page_change(page, cx);
        let was = self.navigation.page;
        self.navigation.page = page;
        if page == Page::Multiplayer && was.tab() != Some(Tab::Online) {
            self.rooms.show_room();
        }
        if let Some(tab) = OnlineTab::of(page) {
            crate::features::online::Online::enter(self, cx);
            if self.nav_mp() && self.navigation.online_tab != Some(tab) {
                self.navigation.online_tab = Some(tab);
                tab.save(&self.store.root);
            }
        }
        if page == Page::Shaders {
            self.sync_shaders(true, cx);
        }
        if page == Page::Careers && was != Page::Careers {
            crate::features::career_saves::SavesPage::refresh(self, cx);
        }
        if page == Page::Moderation {
            crate::features::time_trial::TtPage::open(self, cx);
        }
        if page == Page::Store {
            crate::features::store::StorePage::open(self, cx);
        }
        if let Some(i) = self.open_section().filter(|i| !self.section_loaded(*i)) {
            crate::features::sections::AddonViews::fetch(self, Some(i), cx);
        }
        cx.notify();
    }

    pub(super) fn section_loaded(&self, i: usize) -> bool {
        self.library.sections.get(i).is_some_and(|s| {
            self.addon_views
                .views
                .contains_key(&format!("{}/{}", s.addon, s.spec.id))
        })
    }
}
