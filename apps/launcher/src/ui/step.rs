use crate::ui::scale::px;
use crate::ui::theme::Palette;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;

/// One numbered instruction in a dialog: a number badge beside a title and a line of body text.
pub(crate) fn numbered_step(n: u32, title: &'static str, body: &'static str, p: Palette) -> impl IntoElement {
    h_flex()
        .gap(px(12.))
        .items_start()
        .child(
            div()
                .size(px(20.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(p.selected)
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.accent)
                .child(n.to_string()),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .text_color(p.text_muted)
                        .child(body),
                ),
        )
}
