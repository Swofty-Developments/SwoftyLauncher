use super::model::*;
use super::widgets::*;
use super::{Load, Target};
use crate::format::lap_time;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf, trn};

impl Shell {
    pub(crate) fn online_landing(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.online.overview {
            Load::Idle | Load::Loading => return self.skeleton(p, 4, 120.),
            Load::Failed(e) => {
                let e = e.clone();
                return self.failed_view(&e, cx);
            }
            Load::Ready(o) if o.seasons.is_empty() && o.global.is_empty() && o.custom.is_empty() => {
                return self.nothing_yet();
            }
            Load::Ready(_) => {}
        }
        let Load::Ready(o) = &self.online.overview else {
            return div().into_any_element();
        };
        let o = o.clone();
        let mut items: Vec<Target> = Vec::new();
        let me = self.account_name();
        let wide = self.content_width >= 960.;
        let hero = self.tt_hero(&o, me.as_deref(), &mut items, p, cx);
        let seasons = self.season_grid(&o, wide, &mut items, p, cx);
        let custom = self.custom_section(&o, &mut items, p, cx);
        self.online.items = items;
        v_flex()
            .flex_1()
            .min_h_0()
            .scroll_y("online-landing")
            .px(px(28.))
            .pb(px(28.))
            .gap(px(26.))
            .child(hero)
            .child(seasons)
            .children(custom)
            .into_any_element()
    }

    fn tt_hero(
        &mut self,
        o: &Overview,
        me: Option<&str>,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let top: Vec<&RankEntry> = o.global.iter().take(3).collect();
        let cards: Vec<AnyElement> = top
            .iter()
            .map(|r| {
                let idx = self.target_item(items, Target::Profile(r.driver.clone()));
                self.podium_card(r, idx, p, cx)
            })
            .collect();
        let rest: Vec<AnyElement> = o
            .global
            .iter()
            .skip(3)
            .take(7)
            .map(|r| {
                let idx = self.target_item(items, Target::Profile(r.driver.clone()));
                self.rank_row(r, idx, me == Some(r.driver.name.as_str()), p, cx)
            })
            .collect();
        let pinned = me
            .and_then(|m| o.global.iter().find(|r| r.driver.name == m))
            .map(|r| {
                h_flex()
                    .px(px(14.))
                    .py(px(10.))
                    .gap(px(10.))
                    .items_center()
                    .bg(p.selected)
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(trf!(
                                "You: #{rank}, {points} pts",
                                rank = r.rank,
                                points = score_text(r.points)
                            )),
                    )
            });
        panel(p)
            .child(
                h_flex().px(px(18.)).pt(px(16.)).pb(px(4.)).items_center().child(
                    v_flex().child(section_title(tr("GLOBAL RANKING"), p)).child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(tr("Every season counts; racing more seasons scores more.")),
                    ),
                ),
            )
            .child(
                h_flex()
                    .p(px(18.))
                    .gap(px(18.))
                    .items_stretch()
                    .child(h_flex().flex_1().min_w_0().gap(px(12.)).children(cards))
                    .when(self.content_width >= 900., |d| {
                        d.child(
                            v_flex()
                                .w(px(340.))
                                .flex_shrink_0()
                                .rounded(px(10.))
                                .bg(p.background)
                                .border_1()
                                .border_color(p.border_variant)
                                .overflow_hidden()
                                .children(rest),
                        )
                    }),
            )
            .children(pinned)
            .into_any_element()
    }

    fn podium_card(&self, r: &RankEntry, idx: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let driver = r.driver.clone();
        let colour = medal(r.rank, &p);
        v_flex()
            .id(SharedString::from(format!("podium-{}", r.driver.id)))
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(6.))
            .py(px(if r.rank == 1 { 18. } else { 14. }))
            .px(px(10.))
            .rounded(px(12.))
            .border_1()
            .border_color(if self.online.ringed(idx) {
                p.accent
            } else {
                colour.opacity(0.45)
            })
            .bg(colour.opacity(0.08))
            .cursor_pointer()
            .hover(move |s| s.bg(colour.opacity(0.16)))
            .child(
                div()
                    .text_size(px(22.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colour)
                    .child(format!("#{}", r.rank)),
            )
            .child(self.tt_face(&r.driver, if r.rank == 1 { 76. } else { 62. }, p))
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(r.driver.name.clone()),
            )
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child(score_text(r.points)),
            )
            .child(div().text_size(px(11.)).text_color(p.text_muted).child(trf!(
                "{wins} wins - {podiums} podiums",
                wins = r.wins,
                podiums = r.podiums
            )))
            .on_click(cx.listener(move |this, _, _, cx| this.online_go(Target::Profile(driver.clone()), cx)))
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
            .h(px(40.))
            .px(px(12.))
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
                    .w(px(28.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(medal(r.rank, &p))
                    .child(r.rank.to_string()),
            )
            .child(self.tt_face(&r.driver, 24., p))
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
            .on_click(cx.listener(move |this, _, _, cx| this.online_go(Target::Profile(driver.clone()), cx)))
            .into_any_element()
    }

    fn season_grid(
        &mut self,
        o: &Overview,
        wide: bool,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cards: Vec<AnyElement> = o
            .seasons
            .iter()
            .map(|s| {
                let idx = self.target_item(items, Target::Season(s.id.clone(), s.name.clone()));
                self.season_card(s, idx, p, cx)
            })
            .collect();
        v_flex()
            .gap(px(12.))
            .child(section_title(tr("SEASONS"), p))
            .child(
                div()
                    .grid()
                    .grid_cols(if wide { 3 } else { 2 })
                    .gap(px(14.))
                    .children(cards),
            )
            .into_any_element()
    }

    fn season_card(&self, s: &SeasonCard, idx: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let (id, name) = (s.id.clone(), s.name.clone());
        div()
            .id(SharedString::from(format!("season-{}", s.id)))
            .relative()
            .h(px(150.))
            .rounded(px(14.))
            .overflow_hidden()
            .border_1()
            .border_color(if self.online.ringed(idx) {
                p.accent
            } else {
                p.border_variant
            })
            .cursor_pointer()
            .child(self.tt_cover(s.cover.as_ref(), &s.id))
            .child(div().absolute().inset_0().bg(linear_gradient(
                180.,
                linear_color_stop(hsla(0., 0., 0., 0.0), 0.2),
                linear_color_stop(hsla(0., 0., 0., 0.78), 1.),
            )))
            .child(
                v_flex()
                    .absolute()
                    .left(px(16.))
                    .bottom(px(14.))
                    .right(px(16.))
                    .child(
                        h_flex()
                            .items_end()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(20.))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child(s.name.clone()),
                                    )
                                    .child(div().text_size(px(12.)).text_color(hsla(0., 0., 1., 0.8)).child(
                                        format!(
                                            "{} - {}",
                                            trn(s.tracks as usize, "{count} track", "{count} tracks"),
                                            trn(s.drivers as usize, "{count} driver", "{count} drivers")
                                        ),
                                    )),
                            )
                            .child(self.tt_faces(&s.top, 28., p)),
                    ),
            )
            .on_click(
                cx.listener(move |this, _, _, cx| {
                    this.online_go(Target::Season(id.clone(), name.clone()), cx)
                }),
            )
            .into_any_element()
    }

    fn custom_section(
        &mut self,
        o: &Overview,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if o.custom.is_empty() {
            return None;
        }
        let rows: Vec<AnyElement> = o
            .custom
            .iter()
            .map(|t| {
                let idx = self.target_item(items, Target::Board(Box::new(t.clone()), t.name.clone()));
                self.track_row(t, idx, p, cx)
            })
            .collect();
        Some(
            v_flex()
                .gap(px(12.))
                .child(section_title(tr("CUSTOM TRACKS"), p))
                .child(panel(p).children(rows))
                .into_any_element(),
        )
    }

    pub(super) fn track_row(
        &self,
        t: &TrackCard,
        idx: usize,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = t.clone();
        let label = t.name.clone();
        let record = t.record.clone();
        h_flex()
            .id(SharedString::from(format!("track-{idx}-{}", t.key.track)))
            .gap(px(16.))
            .p(px(14.))
            .items_center()
            .border_b_1()
            .border_color(p.border_variant)
            .when(self.online.ringed(idx), |d| d.bg(p.selected))
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(self.layout_art(t.layout.as_ref(), 84., p.accent_hover, p))
            .child(
                v_flex()
                    .w(px(190.))
                    .flex_shrink_0()
                    .gap(px(3.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(t.name.clone()),
                            )
                            .children(self.flag_chip(&t.flag, p)),
                    )
                    .children(record.map(|r| {
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.accent_hover)
                                    .child(lap_time(r.time_ms)),
                            )
                            .child(self.tt_face(&r.driver, 18., p))
                    }))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.text_placeholder)
                            .child(format!(
                                "{} - {}",
                                trn(t.drivers as usize, "{count} driver", "{count} drivers"),
                                trn(t.laps as usize, "{count} lap", "{count} laps")
                            )),
                    ),
            )
            .child(self.podium_strip(t, p))
            .child(
                Icon::new(gpui_kit::assets::IconName::ChevronRight)
                    .size(px(16.))
                    .text_color(p.text_placeholder),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.online_go(Target::Board(Box::new(card.clone()), label.clone()), cx)
            }))
            .into_any_element()
    }

    pub(super) fn podium_strip(&self, t: &TrackCard, p: Palette) -> Div {
        let leader = t.podium.first().map_or(0, |e| e.time_ms);
        h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(10.))
            .children(t.podium.iter().enumerate().map(|(i, e)| {
                let place = i as u32 + 1;
                let colour = medal(place, &p);
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.))
                    .items_center()
                    .p(px(8.))
                    .rounded(px(10.))
                    .bg(colour.opacity(0.07))
                    .border_1()
                    .border_color(colour.opacity(0.3))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(colour)
                            .child(place.to_string()),
                    )
                    .child(self.tt_face(&e.driver, 26., p))
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(e.driver.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.text_muted)
                                    .child(match place {
                                        1 => lap_time(e.time_ms),
                                        _ => gap_text(e.time_ms, leader),
                                    }),
                            ),
                    )
            }))
    }
}
