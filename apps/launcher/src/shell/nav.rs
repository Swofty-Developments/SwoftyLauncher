use gpui_kit::*;
use std::time::Instant;

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
}

impl Shell {
    pub(crate) fn cancel_scroll(&self) {
        self.news_scroll.cancel();
        self.addons.cancel_scroll();
        self.browser.smooth.cancel();
        self.db.rows_motion.cancel();
        self.db.list_motion.cancel();
    }

    pub(crate) fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.navigation.page != page {
            self.cancel_scroll();
        }
        self.db_page_change(page, cx);
        let was = self.navigation.page;
        self.navigation.page = page;
        if page == Page::Online {
            crate::features::online::Online::enter(self, cx);
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
