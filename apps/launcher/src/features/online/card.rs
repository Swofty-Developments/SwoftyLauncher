//! The Time Trial card at the top of the Multiplayer page's rail: the player's place, the first tracks of a season
//! with their records, and the way to the Time Trial page.

use std::collections::HashSet;

use super::frame::{open_link, preview_card};
use super::model::*;
use super::{Load, Online};
use crate::shell::{Page, Shell};
use crate::ui::format::{lap_time, sector_time};
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf};

const ROW_H: f32 = 50.;

impl Shell {
    pub(crate) fn trial_card(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let open = open_link("tt-card-open", tr("Open Time Trial"), p)
            .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Online, cx)));
        let season = match &self.online.overview {
            Load::Idle | Load::Loading => Err(card_skeleton(3, p)),
            Load::Failed(e) => Err(card_note(e.to_string(), p)),
            Load::Ready(o) => hub_season(
                &o.seasons,
                self.online.hub_season.as_deref(),
                self.online.recents.list(),
            )
            .cloned()
            .ok_or_else(|| card_note(tr("Nobody has set a lap yet.").to_owned(), p)),
        };
        let body = match season {
            Ok(season) => self.card_season(&season, p, cx),
            Err(note) => note,
        };
        preview_card(tr("TIME TRIAL"), open, p)
            .children(self.card_place(p))
            .child(body)
            .into_any_element()
    }

    /// The player's place in the global ranking, once they are signed in and ranked.
    fn card_place(&self, p: Palette) -> Option<Div> {
        let me = self.account.display_name().and(self.online.me.as_ref())?;
        let entry = me.entry.as_ref()?;
        Some(
            h_flex()
                .h(px(48.))
                .px(px(14.))
                .gap(px(10.))
                .items_center()
                .border_b_1()
                .border_color(p.border_variant)
                .child(self.tt_face(&me.driver, 26., p))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(trf!("Global rank #{rank}", rank = entry.rank)),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{} {}", score_text(entry.points), tr("pts"))),
                ),
        )
    }

    fn card_season(&mut self, season: &SeasonCard, p: Palette, cx: &mut Context<Self>) -> Div {
        Online::load_season(self, &season.id, cx);
        let head = h_flex()
            .h(px(40.))
            .px(px(14.))
            .gap(px(10.))
            .items_center()
            .child(self.season_icon(&season.id, season.cover.as_ref(), 22., p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(season.name.clone()),
            );
        let tracks: Result<Vec<TrackCard>, Div> = match self.online.seasons.get(&season.id) {
            None | Some(Load::Idle | Load::Loading) => Err(card_skeleton(3, p)),
            Some(Load::Failed(e)) => Err(card_note(e.to_string(), p)),
            Some(Load::Ready(page)) => {
                let mine: HashSet<BoardKey> = self.online.my_times.keys().cloned().collect();
                Ok(TrackFilter::default()
                    .apply(&page.tracks, &mine)
                    .into_iter()
                    .take(CARD_TRACKS)
                    .cloned()
                    .collect())
            }
        };
        let rows: Vec<AnyElement> = match tracks {
            Ok(tracks) => tracks
                .iter()
                .map(|t| self.card_track(t, &season.id, p, cx))
                .collect(),
            Err(note) => vec![note.into_any_element()],
        };
        v_flex().pb(px(6.)).child(head).children(rows)
    }

    /// A track's record and holder, with the player's best against it. Opens the track on the Time Trial page.
    fn card_track(&self, t: &TrackCard, season: &str, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let gap = own_gap(
            self.online.my_times.get(&t.key).copied(),
            t.record.as_ref().map(|r| r.time_ms),
        );
        let (gap_text, gap_colour) = match gap {
            OwnGap::NoLap => ("\u{2014}".to_owned(), p.text_placeholder),
            OwnGap::Record => (tr("Your record").to_owned(), p.success),
            OwnGap::Behind(ms) => (format!("+{}", sector_time(ms)), p.text_muted),
        };
        let (season, key) = (season.to_owned(), t.key.clone());
        h_flex()
            .id(SharedString::from(format!(
                "tt-card-{}-{}",
                t.key.track, t.key.weather
            )))
            .h(px(ROW_H))
            .px(px(14.))
            .gap(px(10.))
            .items_center()
            .border_t_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t.name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(p.text_muted)
                            .child(
                                t.record
                                    .as_ref()
                                    .map(|r| r.driver.name.clone())
                                    .unwrap_or_default(),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .items_end()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.accent_hover)
                            .when_some(t.record.as_ref(), |d, r| d.child(lap_time(r.time_ms))),
                    )
                    .child(div().text_size(px(11.)).text_color(gap_colour).child(gap_text)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                Online::show_track(this, season.clone(), key.clone(), cx);
                this.open_page(Page::Online, cx);
            }))
            .into_any_element()
    }
}

fn card_note(text: String, p: Palette) -> Div {
    div()
        .px(px(14.))
        .py(px(16.))
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(text)
}

fn card_skeleton(rows: usize, p: Palette) -> Div {
    v_flex().children((0..rows).map(|_| {
        h_flex()
            .h(px(ROW_H))
            .px(px(14.))
            .gap(px(10.))
            .items_center()
            .border_t_1()
            .border_color(p.border_variant)
            .child(div().h(px(12.)).w(px(140.)).rounded(px(4.)).bg(p.elevated))
            .child(div().flex_1())
            .child(div().h(px(12.)).w(px(60.)).rounded(px(4.)).bg(p.elevated))
    }))
}
