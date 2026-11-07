use crate::scale::px;
use crate::theme::palette;
use gpui_kit::component::{WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::Instant;

use super::{Page, Shell};

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();
        self.painted = Instant::now();
        let p = palette(cx);
        if let Some(wizard) = self.onboarding_view(p, cx) {
            return wizard;
        }
        let wide = window.viewport_size().width > px(1500.);
        let short = window.viewport_size().height < px(760.);
        self.short = short;
        self.content_width =
            crate::scale::content_width(f32::from(window.viewport_size().width), crate::scale::current());
        self.sync_sliders(window, cx);
        self.sync_blocks(window, cx);
        self.db_pending_tab(window, cx);
        if self.page == Page::Database {
            self.sync_db(window, cx);
        }
        for note in std::mem::take(&mut self.db.toasts) {
            window.push_notification(note, cx);
        }
        for note in std::mem::take(&mut self.browser.toasts) {
            window.push_notification(note, cx);
        }
        for note in std::mem::take(&mut self.mp_toasts) {
            window.push_notification(note, cx);
        }
        let content = match self.page {
            Page::News => self.news_page(p, short, cx),
            Page::Importer => self.importer_page(p, cx),
            Page::Tools => match self.developer_section(p, cx) {
                Some(dev) => v_flex()
                    .size_full()
                    .child(div().flex_1().min_h_0().child(self.tools_page(p, window, cx)))
                    .child(div().px(px(crate::pages::GUTTER)).pb(px(16.)).child(dev))
                    .into_any_element(),
                None => self.tools_page(p, window, cx),
            },
            Page::Addons => self.addons_page(p, cx),
            Page::Browser => self.browser_page(p, cx),
            Page::Downloads => self.downloads_page(p, cx),
            Page::Shaders => self.shaders.clone().into_any_element(),
            Page::Database => self.db_page(p, cx),
            Page::Profiles => self.profiles_page(p, wide, cx),
            Page::Careers => self.careers_page(p, wide, cx),
            Page::Multiplayer => self.multiplayer_page(p, cx),
            Page::Settings => self.settings_page(p, cx),
            Page::Section(i) => self.section_page(i, p, cx),
        };
        if self.dropping && !cx.has_active_drag() {
            self.dropping = false;
        }
        let page = self.page_name();
        let tree = v_flex()
            .id("shell")
            .relative()
            .size_full()
            .on_drag_move::<ExternalPaths>(cx.listener(|this, _, _, cx| {
                if !this.dropping {
                    this.dropping = true;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_paths(paths.paths().to_vec(), window, cx)
            }))
            .bg(p.background)
            .text_color(p.text)
            .text_size(px(14.))
            .child(self.title_bar(p, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .bg(p.chrome)
                    .child(self.sidebar(p, short, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .overflow_hidden()
                            .rounded_tl(px(16.))
                            .bg(p.background)
                            .child(content),
                    ),
            )
            .children(self.update_bar(p))
            .child(self.play_bar(p, wide, cx))
            .children(self.progress_card(p))
            .children(self.download_dock(p, window, cx))
            .children(self.task_dialog(p, window, cx))
            .children(self.crash_overlay(p, cx))
            .children(self.data_move_overlay(p, cx))
            .children(self.found_data_overlay(p, cx))
            .when(self.dropping, |d| d.child(self.drop_overlay(p)));
        self.note_slow("render", page, started);
        tree
    }
}
