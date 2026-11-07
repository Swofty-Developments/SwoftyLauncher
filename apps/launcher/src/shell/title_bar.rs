use crate::scale::px;
use crate::theme::Palette;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Sizable, h_flex};
use gpui_kit::*;

use super::Shell;

impl Shell {
    pub(super) fn title_bar(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let control: AnyElement = match &self.mp {
            Some(sess) => h_flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text)
                        .child(sess.name.clone()),
                )
                .child(
                    Button::new("mp-logout")
                        .ghost()
                        .small()
                        .label("Log out")
                        .occlude()
                        .disabled(self.mp_busy)
                        .on_click(cx.listener(|this, _, window, cx| this.mp_sign_out(window, cx))),
                )
                .into_any_element(),
            None => Button::new("mp-login")
                .small()
                .label(if self.mp_busy {
                    "Signing in\u{2026}"
                } else {
                    "Log in"
                })
                .occlude()
                .disabled(self.mp_busy)
                .on_click(cx.listener(|this, _, window, cx| this.mp_sign_in(window, cx)))
                .into_any_element(),
        };
        crate::ui::title_bar(p).child(
            h_flex()
                .flex_1()
                .justify_end()
                .items_center()
                .gap(px(12.))
                .pr(px(8.))
                .child(control)
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                ),
        )
    }
}
