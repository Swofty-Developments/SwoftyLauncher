use std::collections::HashSet;

use super::frame::RankFit;
use super::model::*;
use super::widgets::*;
use super::{Load, Online, Target};
use crate::shell::Shell;
use crate::ui::scale::{Hit, px};
use crate::ui::side_scroll;
use crate::ui::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trn};

impl Shell {
    /// The Time Trial page at the root of its trail: the room strip while in a room, Time Trial in the main column,
    /// and the rooms card above the global ranking in the rail.
    pub(super) fn trial_tab(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let width = self.online_main_width();
        let mut items = Vec::new();
        let strip = self.room_strip(p, cx);
        let ad = self.ad_banner(crate::features::ads::Unit::Online, width, p);
        let trial = self.hub_trial(width, ad, &mut items, p, cx);
        let card = self.rooms_card(p, cx);
        Online::rank_ensure(self, "all", cx);
        let ranking = self.rank_column("all", tr("GLOBAL RANKING"), &mut items, p, cx);
        self.online.items = items;
        let main = v_flex().gap(px(20.)).children(strip).child(trial);
        self.online_frame("online-trial", main, card, ranking, RankFit::Fill)
    }

    /// Time Trial's filters over the ad, then a season's tracks with their records, each opening in place onto its
    /// board.
    fn hub_trial(
        &mut self,
        width: f32,
        ad: Option<AnyElement>,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let head = h_flex()
            .flex_wrap()
            .min_h(px(34.))
            .gap_x(px(12.))
            .gap_y(px(8.))
            .items_center()
            .justify_between()
            .child(section_title(tr("TIME TRIAL"), p))
            .child(self.tt_category_bar(self.online.category, false, p, cx));
        let body = match &self.online.overview {
            Load::Idle | Load::Loading => hub_skeleton(4, 42., p),
            Load::Failed(e) => {
                let e = e.clone();
                panel(p)
                    .py(px(20.))
                    .child(self.failed_view(&e, cx))
                    .into_any_element()
            }
            Load::Ready(o) => match landing_empty(o, &self.online.category) {
                Some(LandingEmpty::NothingYet) => {
                    panel(p).py(px(20.)).child(self.nothing_yet()).into_any_element()
                }
                Some(LandingEmpty::Filtered) => panel(p)
                    .py(px(20.))
                    .child(self.no_laps_filtered(cx))
                    .into_any_element(),
                None => {
                    let o = o.clone();
                    self.hub_season_tracks(&o, width, items, p, cx)
                }
            },
        };
        v_flex()
            .gap(px(10.))
            .child(head)
            .children(ad)
            .child(body)
            .into_any_element()
    }

    fn hub_season_tracks(
        &mut self,
        o: &Overview,
        width: f32,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(season) = hub_season(
            &o.seasons,
            self.online.hub_season.as_deref(),
            self.online.recents.list(),
        )
        .cloned() else {
            return div().into_any_element();
        };
        Online::load_season(self, &season.id, cx);
        let chips = h_flex()
            .id("hub-seasons")
            .gap(px(8.))
            .pb(px(2.))
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .children(
                o.seasons
                    .iter()
                    .map(|s| self.season_chip(s, s.id == season.id, p, cx)),
            );
        let chips = side_scroll("hub-seasons-more", chips, &self.online.chips, p, cx);
        let open_season = {
            let (id, name) = (season.id.clone(), season.name.clone());
            Button::new("hub-open-season")
                .ghost()
                .tap_sm()
                .label(tr("Open season"))
                .icon(IconName::ChevronRight)
                .on_click(cx.listener(move |this, _, _, cx| {
                    Online::go(this, Target::Season(id.clone(), name.clone()), cx)
                }))
        };
        let line = h_flex()
            .gap(px(12.))
            .items_center()
            .child(self.season_icon(&season.id, season.cover.as_ref(), 36., p))
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(season.name.clone()),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_muted).child(format!(
                        "{} \u{b7} {}",
                        trn(season.tracks as usize, "{count} track", "{count} tracks"),
                        trn(season.drivers as usize, "{count} driver", "{count} drivers")
                    ))),
            )
            .child(self.tt_faces(&season.top, 24., p))
            .child(div().flex_1())
            .child(open_season);
        let tracks = match self.online.seasons.get(&season.id) {
            None | Some(Load::Idle | Load::Loading) => hub_skeleton(6, 42., p),
            Some(Load::Failed(e)) => panel(p)
                .py(px(20.))
                .child(self.failed_view(&e.clone(), cx))
                .into_any_element(),
            Some(Load::Ready(page)) => {
                let page = page.clone();
                let custom: Vec<TrackCard> = o
                    .custom
                    .iter()
                    .filter(|t| t.key.content == season.id)
                    .cloned()
                    .collect();
                self.hub_track_list(&page.tracks, &custom, width, items, p, cx)
            }
        };
        v_flex()
            .gap(px(12.))
            .child(chips)
            .child(line)
            .child(tracks)
            .into_any_element()
    }

    fn season_chip(&self, s: &SeasonCard, on: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let id = s.id.clone();
        h_flex()
            .id(SharedString::from(format!("hub-season-{}", s.id)))
            .h(px(30.))
            .flex_shrink_0()
            .pl(px(5.))
            .pr(px(11.))
            .gap(px(7.))
            .items_center()
            .rounded(px(15.))
            .border_1()
            .cursor_pointer()
            .text_size(px(12.))
            .map(|d| {
                if on {
                    d.border_color(p.accent)
                        .bg(p.selected)
                        .text_color(p.text)
                        .font_weight(FontWeight::SEMIBOLD)
                } else {
                    d.border_color(p.border_variant)
                        .text_color(p.text_muted)
                        .hover(move |st| st.bg(p.hover).text_color(p.text))
                }
            })
            .child(self.season_icon(&s.id, s.cover.as_ref(), 20., p).rounded_full())
            .child(s.name.clone())
            .on_click(cx.listener(move |this, _, _, cx| Online::pick_season(this, id.clone(), cx)))
            .into_any_element()
    }

    fn hub_track_list(
        &mut self,
        tracks: &[TrackCard],
        custom: &[TrackCard],
        width: f32,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mine_keys: HashSet<BoardKey> = self.online.my_times.keys().cloned().collect();
        let order = TrackFilter::default();
        let shown: Vec<TrackCard> = order
            .apply(tracks, &mine_keys)
            .into_iter()
            .chain(order.apply(custom, &mine_keys))
            .cloned()
            .collect();
        if shown.is_empty() {
            return panel(p)
                .p(px(18.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(tr("No tracks have laps yet."))
                .into_any_element();
        }
        let rows = self.track_rows(&shown, width, true, items, p, cx);
        panel(p).children(rows).into_any_element()
    }
}

fn hub_skeleton(rows: usize, height: f32, p: Palette) -> AnyElement {
    panel(p)
        .children((0..rows).map(|_| {
            h_flex()
                .h(px(height))
                .px(px(14.))
                .gap(px(12.))
                .items_center()
                .border_b_1()
                .border_color(p.border_variant)
                .child(div().size(px(24.)).rounded(px(6.)).bg(p.elevated))
                .child(div().h(px(12.)).w(px(160.)).rounded(px(4.)).bg(p.elevated))
                .child(div().h(px(12.)).w(px(90.)).rounded(px(4.)).bg(p.elevated))
        }))
        .into_any_element()
}
