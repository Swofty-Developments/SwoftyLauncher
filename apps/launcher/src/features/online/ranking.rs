use super::model::*;
use super::widgets::*;
use super::{Online, Target};
use crate::shell::Shell;
use crate::ui::scale::Hit;
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use gpui_kit::SharedString;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf};

pub(super) const RIGHT_W: f32 = 360.;

impl Shell {
    pub(super) fn rank_column(
        &mut self,
        scope: &str,
        title: &'static str,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let feed = self.online.feeds.get(scope).cloned().unwrap_or_default();
        for r in feed.rows.iter().take(3) {
            items.push(Target::Profile(r.driver.clone()));
        }
        let header = self.rank_header(title, p);
        let body: AnyElement = if !feed.started {
            v_flex()
                .children((0..8).map(|_| {
                    div()
                        .h(px(38.))
                        .mx(px(14.))
                        .my(px(2.))
                        .rounded(px(8.))
                        .bg(p.elevated)
                }))
                .into_any_element()
        } else if let (Some(e), true) = (&feed.error, feed.rows.is_empty()) {
            v_flex()
                .p(px(18.))
                .gap(px(10.))
                .items_center()
                .child(div().text_size(px(12.)).text_color(p.text_muted).child(e.clone()))
                .child(
                    gpui_kit::component::button::Button::new(SharedString::from(format!(
                        "rank-retry-{scope}"
                    )))
                    .outline()
                    .tap_sm()
                    .label(tr("Try again"))
                    .on_click({
                        let scope = scope.to_owned();
                        cx.listener(move |this, _, _, cx| {
                            this.online.feeds.remove(&scope);
                            cx.notify();
                        })
                    }),
                )
                .into_any_element()
        } else if feed.rows.is_empty() {
            div()
                .p(px(18.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(tr(
                    "No ranking yet. A track counts once three drivers have set a lap on it.",
                ))
                .into_any_element()
        } else {
            let handle = self.online.handles.entry(scope.to_owned()).or_default().clone();
            let key = scope.to_owned();
            let count = chunk_count(feed.rows.len());
            div()
                .flex_1()
                .min_h_0()
                .child(
                    uniform_list(
                        SharedString::from(format!("rank-list-{scope}")),
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            let feed = this.online.feeds.get(&key).cloned().unwrap_or_default();
                            if let Some(off) = feed.wants_more(3 + range.end * CHUNK + 3) {
                                Online::rank_load(this, &key, off, cx);
                            }
                            let me_id = this.online.me.as_ref().map(|m| m.driver.id.clone());
                            range
                                .map(|chunk| {
                                    let rows: Vec<AnyElement> = chunk_slots(chunk, feed.rows.len())
                                        .into_iter()
                                        .map(|slot| match slot {
                                            Slot::Top(i) => {
                                                let r = &feed.rows[i];
                                                let mine = me_id.as_deref() == Some(r.driver.id.as_str());
                                                this.top_rank_row(r, usize::MAX, mine, p, cx)
                                            }
                                            Slot::Row(i) => {
                                                let r = &feed.rows[i];
                                                let mine = me_id.as_deref() == Some(r.driver.id.as_str());
                                                this.rank_row(r, usize::MAX, mine, p, cx)
                                            }
                                            Slot::Footer => this.rank_footer(&feed, p),
                                        })
                                        .collect();
                                    v_flex().h(px(CHUNK_H)).w_full().children(rows).into_any_element()
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .size_full()
                    .track_scroll(&handle),
                )
                .into_any_element()
        };
        let pinned = self.rank_you(p);
        panel(p)
            .flex_1()
            .min_h_0()
            .child(header)
            .child(body)
            .children(pinned)
            .into_any_element()
    }

    /// The ranking cut short: its first `rows` drivers and the player's own place, for a rail with a card above it.
    pub(crate) fn rank_compact(
        &mut self,
        scope: &str,
        title: &'static str,
        rows: usize,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let feed = self.online.feeds.get(scope).cloned().unwrap_or_default();
        let me_id = self.online.me.as_ref().map(|m| m.driver.id.clone());
        let body: Vec<AnyElement> = if !feed.started {
            (0..rows)
                .map(|_| {
                    div()
                        .h(px(30.))
                        .mx(px(14.))
                        .my(px(4.))
                        .rounded(px(6.))
                        .bg(p.elevated)
                        .into_any_element()
                })
                .collect()
        } else if feed.rows.is_empty() {
            let text = match &feed.error {
                Some(e) => e.clone(),
                None => {
                    tr("No ranking yet. A track counts once three drivers have set a lap on it.").to_owned()
                }
            };
            vec![
                div()
                    .p(px(18.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(text)
                    .into_any_element(),
            ]
        } else {
            feed.rows
                .iter()
                .take(rows)
                .map(|r| {
                    let mine = me_id.as_deref() == Some(r.driver.id.as_str());
                    self.rank_row(r, usize::MAX, mine, p, cx)
                })
                .collect()
        };
        panel(p)
            .flex_shrink_0()
            .child(self.rank_header(title, p))
            .children(body)
            .children(self.rank_you(p))
            .into_any_element()
    }

    fn rank_header(&self, title: &str, p: Palette) -> Div {
        div()
            .h(px(46.))
            .flex()
            .items_center()
            .justify_center()
            .child(section_title(rank_title(title, &self.online.category), p))
    }

    /// The signed-in player's own place, pinned under a ranking.
    fn rank_you(&self, p: Palette) -> Option<Div> {
        let m = self
            .account
            .display_name()
            .and(self.online.me.as_ref())
            .and_then(|m| m.entry.as_ref())?;
        Some(
            h_flex()
                .h(px(46.))
                .px(px(14.))
                .gap(px(10.))
                .items_center()
                .bg(p.selected)
                .border_t_1()
                .border_color(p.border)
                .child(self.tt_face(&m.driver, 26., p))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(trf!("You: #{rank}", rank = m.rank)),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{} {}", score_text(m.points), tr("pts"))),
                ),
        )
    }

    fn rank_footer(&self, feed: &Feed, p: Palette) -> AnyElement {
        let text: AnyElement = if feed.error.is_some() {
            div()
                .text_size(px(12.))
                .text_color(p.error)
                .child(tr("Couldn't load more"))
                .into_any_element()
        } else if feed.finished() {
            div()
                .text_size(px(11.))
                .text_color(p.text_placeholder)
                .child(tr("End of the ranking"))
                .into_any_element()
        } else {
            div()
                .h(px(18.))
                .w(px(180.))
                .rounded(px(6.))
                .bg(p.elevated)
                .into_any_element()
        };
        div()
            .h(px(38.))
            .flex()
            .items_center()
            .justify_center()
            .child(text)
            .into_any_element()
    }

    fn top_rank_row(
        &self,
        r: &RankEntry,
        idx: usize,
        mine: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let driver = r.driver.clone();
        let colour = medal(r.rank, &p);
        h_flex()
            .id(SharedString::from(format!("top-{idx}-{}", r.driver.id)))
            .h(px(TOP_ROW_H))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .bg(colour.opacity(if mine { 0.18 } else { 0.08 }))
            .border_b_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .when(self.online.ringed(idx), |d| d.border_1().border_color(p.accent))
            .hover(move |s| s.bg(colour.opacity(0.16)))
            .child(
                div()
                    .w(px(30.))
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colour)
                    .child(format!("#{}", r.rank)),
            )
            .child(self.tt_face(&r.driver, 42., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(r.driver.name.clone()),
                    )
                    .child(div().text_size(px(11.)).text_color(p.text_muted).child(trf!(
                        "{wins} wins - {podiums} podiums",
                        wins = r.wins,
                        podiums = r.podiums
                    ))),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::BOLD)
                    .child(score_text(r.points)),
            )
            .on_click(
                cx.listener(move |this, _, _, cx| Online::go(this, Target::Profile(driver.clone()), cx)),
            )
            .into_any_element()
    }

    pub(super) fn rank_row(
        &self,
        r: &RankEntry,
        idx: usize,
        mine: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let driver = r.driver.clone();
        h_flex()
            .id(SharedString::from(format!("rank-{idx}-{}", r.driver.id)))
            .w_full()
            .h(px(ROW_H))
            .px(px(14.))
            .gap(px(10.))
            .items_center()
            .border_b_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .when(mine, |d| d.bg(p.selected))
            .when(self.online.ringed(idx), |d| d.border_1().border_color(p.accent))
            .hover(move |s| s.bg(p.hover))
            .child(
                div()
                    .w(px(30.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(medal(r.rank, &p))
                    .child(r.rank.to_string()),
            )
            .child(self.tt_face(&r.driver, 22., p))
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
                    .child(score_text(r.points)),
            )
            .on_click(
                cx.listener(move |this, _, _, cx| Online::go(this, Target::Profile(driver.clone()), cx)),
            )
            .into_any_element()
    }
}

fn rank_title(title: &str, cat: &Category) -> String {
    match cat.is_default() {
        true => title.to_owned(),
        false => format!("{title} \u{b7} {}", cat.suffix()),
    }
}
