use super::super::model::*;
use super::super::widgets::*;
use super::super::{Load, Online, Target};
use crate::shell::Shell;
use crate::ui::format::lap_time;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf, trn};

/// From this width a track open in place lists its laps in two columns.
const TWO_COLUMNS: f32 = 720.;
/// The widest a track open in place lays out its laps, so times stay near names on a wide window.
const MAX_WIDTH: f32 = 920.;
const PEEK_ROW_H: f32 = 30.;

impl Shell {
    /// A track open in place on the hub: the top of its board, this player's lap on it, and the way to the board.
    pub(super) fn peek_panel(
        &mut self,
        t: &TrackCard,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = match self.online.peek.page(&t.key) {
            None | Some(Load::Idle | Load::Loading) => v_flex()
                .gap(px(6.))
                .children((0..4).map(|_| div().h(px(PEEK_ROW_H - 6.)).rounded(px(6.)).bg(p.elevated)))
                .into_any_element(),
            Some(Load::Failed(e)) => {
                let key = t.key.clone();
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .text_color(p.error)
                            .child(e.message.clone()),
                    )
                    .child(
                        Button::new("peek-retry")
                            .outline()
                            .tap_sm()
                            .label(tr("Try again"))
                            .on_click(
                                cx.listener(move |this, _, _, cx| Online::load_peek(this, key.clone(), cx)),
                            ),
                    )
                    .into_any_element()
            }
            Some(Load::Ready(page)) => {
                let page = page.clone();
                self.peek_board(&page, width, p)
            }
        };
        let card = t.clone();
        let label = t.name.clone();
        let footer = h_flex()
            .pt(px(8.))
            .gap(px(12.))
            .items_center()
            .children(self.peek_mine(t, p))
            .child(div().flex_1())
            .child(
                Button::new("peek-open-board")
                    .outline()
                    .tap_sm()
                    .label(tr("Open full board"))
                    .icon(IconName::ChevronRight)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        Online::go(this, Target::Board(Box::new(card.clone()), label.clone()), cx)
                    })),
            );
        v_flex()
            .px(px(14.))
            .pt(px(10.))
            .pb(px(12.))
            .bg(p.background)
            .border_b_1()
            .border_color(p.border_variant)
            .child(v_flex().w_full().max_w(px(MAX_WIDTH)).child(body).child(footer))
            .into_any_element()
    }

    fn peek_board(&self, page: &BoardPage, width: f32, p: Palette) -> AnyElement {
        if page.rows.is_empty() {
            return div()
                .py(px(6.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(if self.online.category.is_default() {
                    tr("No laps on this board yet.")
                } else {
                    tr("No laps with these filters. Change the assists or input above.")
                })
                .into_any_element();
        }
        let leader = page.rows.first().map_or(0, |r| r.time_ms);
        let own = self.own_lap(page).map(|r| r.lap_id.clone());
        let rows: Vec<AnyElement> = page
            .rows
            .iter()
            .take(PEEK_ROWS)
            .map(|r| self.peek_row(r, leader, own.as_deref() == Some(r.lap_id.as_str()), p))
            .collect();
        if width.min(MAX_WIDTH) < TWO_COLUMNS || rows.len() <= PEEK_ROWS / 2 {
            return v_flex().children(rows).into_any_element();
        }
        let mut rows = rows.into_iter();
        let left: Vec<AnyElement> = rows.by_ref().take(PEEK_ROWS / 2).collect();
        h_flex()
            .gap(px(28.))
            .items_start()
            .child(v_flex().flex_1().min_w_0().children(left))
            .child(v_flex().flex_1().min_w_0().children(rows))
            .into_any_element()
    }

    fn peek_row(&self, r: &Row, leader: u64, mine: bool, p: Palette) -> AnyElement {
        h_flex()
            .h(px(PEEK_ROW_H))
            .px(px(6.))
            .gap(px(10.))
            .items_center()
            .rounded(px(6.))
            .when(mine, |d| d.bg(p.selected))
            .child(
                div()
                    .w(px(24.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(medal(r.rank, &p))
                    .child(r.rank.to_string()),
            )
            .child(self.tt_face(&r.driver, 20., p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(r.driver.name.clone()),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(lap_time(r.time_ms)),
            )
            .child(
                div()
                    .w(px(64.))
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(gap_text(r.time_ms, leader)),
            )
            .into_any_element()
    }

    /// This player's lap on `page`, which the highlighted and pinned rows and "Your best" read.
    pub(crate) fn own_lap<'a>(&self, page: &'a BoardPage) -> Option<&'a Row> {
        self.account.display_name()?;
        let me = self.online.me.as_ref().map(|m| m.driver.id.as_str());
        own_row(&page.rows, page.mine.as_ref(), me)
    }

    /// This player's lap on the open track, from its board or, before that loads, from their profile.
    fn peek_mine(&self, t: &TrackCard, p: Palette) -> Option<AnyElement> {
        self.account.display_name()?;
        let board = match self.online.peek.page(&t.key) {
            Some(Load::Ready(page)) => Some(page),
            _ => None,
        };
        let leader = board.and_then(|b| b.rows.first()).map(|r| r.time_ms);
        let mine = board.and_then(|b| self.own_lap(b));
        let text = match (mine, self.online.my_times.get(&t.key)) {
            (Some(m), _) => {
                let gap = leader.map(|l| gap_text(m.time_ms, l)).unwrap_or_default();
                trf!(
                    "Your best: P{rank} \u{b7} {time} {gap}",
                    rank = m.rank,
                    time = lap_time(m.time_ms),
                    gap = gap
                )
            }
            (None, Some(ms)) => trf!("Your best: {time}", time = lap_time(*ms)),
            (None, None) => tr("You haven't set a lap here yet").to_owned(),
        };
        let drivers = trn(t.drivers as usize, "{count} driver", "{count} drivers");
        Some(
            h_flex()
                .gap(px(10.))
                .items_center()
                .text_size(px(13.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if mine.is_some() { p.text } else { p.text_muted })
                        .child(text),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(drivers),
                )
                .into_any_element(),
        )
    }
}
