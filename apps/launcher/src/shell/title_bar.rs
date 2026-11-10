use crate::scale::px;
use crate::theme::Palette;
use gpui_kit::component::h_flex;
use gpui_kit::*;

use super::Shell;

impl Shell {
    pub(super) fn title_bar(&self, p: Palette, _: &mut Context<Self>) -> impl IntoElement {
        crate::ui::title_bar(p).child(
            h_flex()
                .flex_1()
                .justify_end()
                .items_center()
                .gap(px(12.))
                .pr(px(8.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                ),
        )
    }
}
