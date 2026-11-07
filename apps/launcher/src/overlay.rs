use gpui_kit::component::{TitleBar, v_flex};
use gpui_kit::*;

use crate::scale::px;
use crate::theme::Palette;

pub(crate) fn takeover(id: impl Into<ElementId>, p: Palette) -> Stateful<Div> {
    v_flex()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(p.background)
        .text_color(p.text)
        .text_size(px(14.))
}

pub(crate) fn title_bar(p: Palette) -> TitleBar {
    TitleBar::new().bg(p.chrome)
}
