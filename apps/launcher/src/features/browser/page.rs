use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

use super::Browser;
use super::authors::View;
use crate::shell::{Page, Shell};
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;

impl Shell {
    pub(crate) fn browser_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.browser.stale() {
            Browser::fetch_catalog(self, cx);
        }
        let actions = h_flex()
            .gap(px(8.))
            .child(self.stars_status(p, cx))
            .child(self.creators_button(cx))
            .child(
                Button::new("browser-activity")
                    .map(|b| {
                        if self.browser.activity_open {
                            b.outline()
                        } else {
                            b.ghost()
                        }
                    })
                    .tap_sm()
                    .icon(IconName::Activity)
                    .label(tr("Activity"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.browser.activity_open = !this.browser.activity_open;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("browser-refresh")
                    .ghost()
                    .tap_sm()
                    .icon(IconName::RefreshCw)
                    .label(tr("Refresh"))
                    .loading(self.browser.feed.fetching)
                    .disabled(self.browser.feed.fetching)
                    .on_click(cx.listener(|this, _, _, cx| Browser::fetch_catalog(this, cx))),
            );
        let (content, ad) = match self.browser.view.clone() {
            View::Addon(id) if self.browser.find(&id).is_some() => (self.browser_detail(&id, p, cx), None),
            View::Author(id) => (self.author_page(&id, p, cx), None),
            View::Creators => (self.creators_page(p, cx), None),
            _ => (
                self.browser_grid(p, cx),
                self.ad_header(crate::features::ads::Unit::Discover, p),
            ),
        };
        v_flex()
            .size_full()
            .relative()
            .child(
                self.header(Page::Browser.label(), self.browser.subtitle(), actions, p)
                    .middle(ad),
            )
            .children(self.trouble_banner(p, cx))
            .child(content)
            .when(self.browser.activity_open, |d| {
                d.child(self.activity_panel(p, cx))
            })
            .into_any_element()
    }
}
