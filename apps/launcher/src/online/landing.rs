use super::model::*;
use super::widgets::*;
use super::{Load, Target};
use crate::format::lap_time;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf, trn};

pub(super) const RIGHT_W: f32 = 360.;

impl Shell {
    pub(crate) fn online_landing(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.online.overview {
            Load::Idle | Load::Loading => return self.skeleton(p, 5, 76.),
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
        let wide = self.content_width >= 860.;
        let seasons: Vec<AnyElement> = o
            .seasons
            .iter()
            .map(|s| {
                let idx = self.target_item(&mut items, Target::Season(s.id.clone(), s.name.clone()));
                self.season_row(s, idx, p, cx)
            })
            .collect();
        let custom: Vec<AnyElement> = o
            .custom
            .iter()
            .map(|t| {
                let idx = self.target_item(&mut items, Target::Board(Box::new(t.clone()), t.name.clone()));
                self.thin_track_row(t, idx, None, p, cx)
            })
            .collect();
        let ranking = self.rank_panel(
            tr("GLOBAL RANKING"),
            tr("Every season counts; racing more seasons scores more."),
            &o.global,
            me.as_deref(),
            &mut items,
            p,
            cx,
        );
        self.online.items = items;
        let left = v_flex()
            .gap(px(14.))
            .child(section_title(tr("SEASONS"), p))
            .child(panel(p).children(seasons))
            .when(!custom.is_empty(), |d| {
                d.child(section_title(tr("CUSTOM TRACKS"), p))
                    .child(panel(p).children(custom))
            });
        if wide {
            h_flex()
                .flex_1()
                .min_h_0()
                .px(px(28.))
                .gap(px(20.))
                .items_start()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .scroll_y("online-seasons")
                        .pb(px(28.))
                        .child(left),
                )
                .child(
                    v_flex()
                        .w(px(RIGHT_W))
                        .flex_shrink_0()
                        .h_full()
                        .scroll_y("online-ranking")
                        .pb(px(28.))
                        .child(ranking),
                )
                .into_any_element()
        } else {
            v_flex()
                .flex_1()
                .min_h_0()
                .scroll_y("online-landing")
                .px(px(28.))
                .pb(px(28.))
                .gap(px(20.))
                .child(left)
                .child(ranking)
                .into_any_element()
        }
    }

    fn season_row(&self, s: &SeasonCard, idx: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let (id, name) = (s.id.clone(), s.name.clone());
        h_flex()
            .id(SharedString::from(format!("season-{}", s.id)))
            .h(px(84.))
            .px(px(16.))
            .gap(px(16.))
            .items_center()
            .border_b_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .when(self.online.ringed(idx), |d| d.bg(p.selected))
            .hover(move |st| st.bg(p.hover))
            .child(self.season_icon(&s.id, s.cover.as_ref(), 56., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(s.name.clone()),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_muted).child(format!(
                        "{} - {}",
                        trn(s.tracks as usize, "{count} track", "{count} tracks"),
                        trn(s.drivers as usize, "{count} driver", "{count} drivers")
                    ))),
            )
            .child(self.tt_faces(&s.top, 30., p))
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(16.))
                    .text_color(p.text_placeholder),
            )
            .on_click(
                cx.listener(move |this, _, _, cx| {
                    this.online_go(Target::Season(id.clone(), name.clone()), cx)
                }),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn rank_panel(
        &self,
        title: &'static str,
        sub: &'static str,
        rows: &[RankEntry],
        me: Option<&str>,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut built: Vec<AnyElement> = Vec::new();
        for r in rows {
            let idx = items.len();
            items.push(Target::Profile(r.driver.clone()));
            built.push(if r.rank <= 3 {
                self.top_rank_row(r, idx, me == Some(r.driver.name.as_str()), p, cx)
            } else {
                self.rank_row(r, idx, me == Some(r.driver.name.as_str()), p, cx)
            });
        }
        let pinned = me
            .and_then(|m| rows.iter().find(|r| r.driver.name == m))
            .map(|r| {
                div()
                    .px(px(14.))
                    .py(px(9.))
                    .bg(p.selected)
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(trf!(
                        "You: #{rank}, {points} pts",
                        rank = r.rank,
                        points = score_text(r.points)
                    ))
            });
        panel(p)
            .child(
                v_flex()
                    .px(px(14.))
                    .pt(px(14.))
                    .pb(px(8.))
                    .child(section_title(title, p))
                    .child(div().text_size(px(11.)).text_color(p.text_placeholder).child(sub)),
            )
            .children(pinned)
            .children(built)
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
            .h(px(64.))
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
            .h(px(38.))
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
            .on_click(cx.listener(move |this, _, _, cx| this.online_go(Target::Profile(driver.clone()), cx)))
            .into_any_element()
    }

    pub(super) fn thin_track_row(
        &self,
        t: &TrackCard,
        idx: usize,
        mine: Option<u64>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = t.clone();
        let label = t.name.clone();
        h_flex()
            .id(SharedString::from(format!(
                "track-{idx}-{}-{}",
                t.key.track, t.key.weather
            )))
            .h(px(42.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .border_b_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .when(self.online.ringed(idx), |d| d.bg(p.selected))
            .hover(move |s| s.bg(p.hover))
            .child(self.layout_art(t.layout.as_ref(), 28., p.accent_hover, p))
            .child(
                h_flex()
                    .w(px(200.))
                    .flex_shrink_0()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t.name.clone()),
                    )
                    .children(self.flag_chip(&t.flag, p)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.))
                    .items_center()
                    .children(t.record.as_ref().map(|r| {
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.accent_hover)
                                    .child(lap_time(r.time_ms)),
                            )
                            .child(self.tt_face(&r.driver, 20., p))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(p.text_muted)
                                    .child(r.driver.name.clone()),
                            )
                    }))
                    .when(t.record.is_none(), |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .child(tr("No ranked time yet")),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(150.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(format!(
                        "{} - {}",
                        trn(t.drivers as usize, "{count} driver", "{count} drivers"),
                        trn(t.laps as usize, "{count} lap", "{count} laps")
                    )),
            )
            .child(
                div()
                    .w(px(84.))
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(mine.map(lap_time).unwrap_or_default()),
            )
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(p.text_placeholder),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.online_go(Target::Board(Box::new(card.clone()), label.clone()), cx)
            }))
            .into_any_element()
    }
}
