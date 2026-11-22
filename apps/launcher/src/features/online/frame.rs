//! The Multiplayer page's frame: its main column on the left, and a rail on the right that opens with a card
//! previewing Time Trial, above a short global ranking.

use super::ranking::RIGHT_W;
use super::widgets::panel;
use crate::shell::Shell;
use crate::ui::scale::px;
use crate::ui::theme::{Palette, ScrollArea};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::*;

/// From this content width the rail sits beside the main column rather than under it.
const WIDE: f32 = 1000.;
const SIDE: f32 = 28.;
const GAP: f32 = 20.;

impl Shell {
    fn online_wide(&self) -> bool {
        self.layout.content_width >= WIDE
    }

    /// The width of the Multiplayer page's main column.
    pub(crate) fn online_main_width(&self) -> f32 {
        self.layout.content_width - 2. * SIDE - if self.online_wide() { GAP + RIGHT_W } else { 0. }
    }

    /// `main` with the rail beside it, or below it on a narrow window with the card first.
    pub(crate) fn online_frame(
        &self,
        id: &'static str,
        main: impl IntoElement,
        card: AnyElement,
        ranking: AnyElement,
    ) -> AnyElement {
        if !self.online_wide() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .scroll_y(id)
                .px(px(SIDE))
                .pb(px(28.))
                .gap(px(24.))
                .child(main)
                .child(card)
                .child(ranking)
                .into_any_element();
        }
        let rail = v_flex()
            .w(px(RIGHT_W))
            .flex_shrink_0()
            .h_full()
            .scroll_y(SharedString::from(format!("{id}-rail")))
            .child(v_flex().gap(px(16.)).pb(px(20.)).child(card).child(ranking));
        h_flex()
            .flex_1()
            .min_h_0()
            .px(px(SIDE))
            .gap(px(GAP))
            .items_start()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .scroll_y(id)
                    .pr(px(12.))
                    .pb(px(28.))
                    .child(main),
            )
            .child(rail)
            .into_any_element()
    }
}

/// A card at the top of the Multiplayer page's rail that previews Time Trial: its title and the way there over
/// `body`.
pub(crate) fn preview_card(title: impl Into<SharedString>, open: impl IntoElement, p: Palette) -> Div {
    panel(p).flex_shrink_0().child(
        h_flex()
            .h(px(44.))
            .px(px(14.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(p.border_variant)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text_muted)
                    .child(title.into()),
            )
            .child(open),
    )
}

/// "Open <page> ›", the way from a preview card to the page it previews.
pub(crate) fn open_link(id: &'static str, label: &'static str, p: Palette) -> Stateful<Div> {
    h_flex()
        .id(id)
        .flex_shrink_0()
        .gap(px(2.))
        .items_center()
        .cursor_pointer()
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.accent_hover)
        .hover(move |s| s.text_color(p.text))
        .child(label)
        .child(Icon::new(IconName::ChevronRight).size(px(14.)))
}
