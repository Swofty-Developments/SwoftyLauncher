use crate::scale::px;
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::*;
use swofty_i18n::tr;

use super::{Page, Shell};

impl Shell {
    pub(crate) fn cancel_scroll(&self) {
        self.news_scroll.cancel();
        self.addons.cancel_scroll();
        self.browser.smooth.cancel();
        self.db.rows_motion.cancel();
        self.db.list_motion.cancel();
    }

    pub(crate) fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.cancel_scroll();
        }
        self.db_page_change(page, cx);
        let was = self.page;
        self.page = page;
        if page == Page::Online {
            self.online_enter(cx);
        }
        if page == Page::Shaders {
            self.sync_shaders(true, cx);
        }
        if page == Page::Careers && was != Page::Careers {
            self.refresh_saves_in_background(cx);
        }
        if let Some(i) = self.open_section().filter(|i| !self.section_loaded(*i)) {
            self.fetch_views(Some(i), cx);
        }
        cx.notify();
    }

    pub(super) fn section_loaded(&self, i: usize) -> bool {
        self.library
            .sections
            .get(i)
            .is_some_and(|s| self.views.contains_key(&format!("{}/{}", s.addon, s.spec.id)))
    }
}

impl Shell {
    pub(super) fn profile_pointer(&self, p: Palette) -> Div {
        h_flex()
            .absolute()
            .right(px(204.))
            .bottom(px(crate::play_bar::HEIGHT + 6.))
            .w(px(260.))
            .gap(px(8.))
            .items_center()
            .px(px(10.))
            .py(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.accent)
            .bg(p.selected)
            .text_size(px(12.))
            .text_color(p.text)
            .child(
                Icon::new(IconName::ArrowDown)
                    .size(px(14.))
                    .flex_shrink_0()
                    .text_color(p.accent),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(tr("Switch profiles here to see their saves")),
            )
    }
}
