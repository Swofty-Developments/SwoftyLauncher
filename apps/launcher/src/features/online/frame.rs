//! The frame both Online pages share: the page's main column on the left, and a rail on the right that opens with
//! a card previewing the other page, above the global ranking.

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
const NARROW_RANKING_H: f32 = 520.;

/// How the rail's ranking takes its height: the whole ranking fills the rail, a short one keeps its own height.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RankFit {
    Fill,
    Own,
}

impl Shell {
    fn online_wide(&self) -> bool {
        self.layout.content_width >= WIDE
    }

    /// The width of an Online page's main column.
    pub(crate) fn online_main_width(&self) -> f32 {
        self.layout.content_width - 2. * SIDE - if self.online_wide() { GAP + RIGHT_W } else { 0. }
    }

    /// `main` with the rail beside it, or below it on a narrow window with the card first.
    pub(crate) fn online_frame(
        &self,
        id: &'static str,
        main: impl IntoElement,
        card: Option<AnyElement>,
        ranking: AnyElement,
        fit: RankFit,
    ) -> AnyElement {
        if !self.online_wide() {
            let ranking = match fit {
                RankFit::Fill => div()
                    .h(px(NARROW_RANKING_H))
                    .flex()
                    .flex_col()
                    .child(ranking)
                    .into_any_element(),
                RankFit::Own => ranking,
            };
            return v_flex()
                .flex_1()
                .min_h_0()
                .scroll_y(id)
                .px(px(SIDE))
                .pb(px(28.))
                .gap(px(24.))
                .child(main)
                .children(card)
                .child(ranking)
                .into_any_element();
        }
        let rail = match fit {
            RankFit::Fill => v_flex()
                .w(px(RIGHT_W))
                .flex_shrink_0()
                .h_full()
                .gap(px(16.))
                .pb(px(20.))
                .children(card)
                .child(v_flex().flex_1().min_h_0().child(ranking))
                .into_any_element(),
            RankFit::Own => v_flex()
                .w(px(RIGHT_W))
                .flex_shrink_0()
                .h_full()
                .scroll_y(SharedString::from(format!("{id}-rail")))
                .child(v_flex().gap(px(16.)).pb(px(20.)).children(card).child(ranking))
                .into_any_element(),
        };
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

/// A card at the top of an Online page's rail that previews the other page: its title and the way there over
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
