//! The Back button of a page opened from a list, and the row it heads. The row
//! sits above the part of the page that scrolls, so Back stays in view.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::h_flex;
use gpui_kit::*;
use swofty_i18n::tr;

use crate::ui::scale::px;
use crate::ui::theme::Palette;

const BACK_H: f32 = 36.;

pub(crate) fn back_button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .outline()
        .h(px(BACK_H))
        .px(px(14.))
        .flex_shrink_0()
        .icon(IconName::ArrowLeft)
        .label(tr("Back"))
}

/// Back, a divider, then where the page is.
pub(crate) fn back_row(
    back: impl IntoElement,
    place: impl IntoIterator<Item = AnyElement>,
    p: Palette,
) -> Div {
    h_flex()
        .gap(px(14.))
        .flex_shrink_0()
        .items_center()
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(back)
        .child(div().w(px(1.)).h(px(14.)).flex_shrink_0().bg(p.border))
        .child(
            h_flex()
                .flex_1()
                .gap(px(6.))
                .min_w_0()
                .overflow_hidden()
                .children(place),
        )
}
