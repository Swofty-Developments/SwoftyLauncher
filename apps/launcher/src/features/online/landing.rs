//! The Time Trial page's root as in 1.10, for a player without access to the rooms: every season, each opening its
//! own page, beside the global ranking.

use super::model::*;
use super::ranking::RIGHT_W;
use super::widgets::*;
use super::{Load, Online, Target};
use crate::shell::Shell;
use crate::ui::scale::px;
use crate::ui::theme::{Palette, ScrollArea};
use gpui_kit::SharedString;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trn};

impl Shell {
    pub(crate) fn online_landing(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.online.overview {
            Load::Idle | Load::Loading => return self.skeleton(p, 5, 76.),
            Load::Failed(e) => {
                let e = e.clone();
                return self.failed_view(&e, cx);
            }
            Load::Ready(o) => match landing_empty(o, &self.online.category) {
                Some(LandingEmpty::NothingYet) => {
                    self.online.items = Vec::new();
                    return self.nothing_yet();
                }
                Some(LandingEmpty::Filtered) => {
                    self.online.items = Vec::new();
                    return v_flex()
                        .flex_1()
                        .min_h_0()
                        .px(px(28.))
                        .gap(px(10.))
                        .child(h_flex().h(px(34.)).items_center().child(self.tt_category_bar(
                            self.online.category,
                            false,
                            p,
                            cx,
                        )))
                        .child(self.no_laps_filtered(cx))
                        .into_any_element();
                }
                None => {}
            },
        }
        let Load::Ready(o) = &self.online.overview else {
            return div().into_any_element();
        };
        let o = o.clone();
        let mut items: Vec<Target> = Vec::new();
        let wide = self.layout.content_width >= 860.;
        let seasons: Vec<AnyElement> = o
            .seasons
            .iter()
            .map(|s| {
                let idx = self.target_item(&mut items, Target::Season(s.id.clone(), s.name.clone()));
                self.season_row(s, idx, p, cx)
            })
            .collect();
        Online::rank_ensure(self, "all", cx);
        let ranking = self.rank_column("all", tr("GLOBAL RANKING"), &mut items, p, cx);
        self.online.items = items;
        let left_width = self.layout.content_width - 56. - if wide { 20. + RIGHT_W } else { 0. };
        let ad = self.ad_banner(crate::features::ads::Unit::Online, left_width, p);
        let left = v_flex()
            .gap(px(10.))
            .child(h_flex().h(px(34.)).items_center().child(self.tt_category_bar(
                self.online.category,
                false,
                p,
                cx,
            )))
            .children(ad)
            .child(section_title(tr("SEASONS"), p))
            .child(panel(p).children(seasons));
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
                        .pb(px(20.))
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
                .child(div().h(px(520.)).flex().flex_col().child(ranking))
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
            .on_click(cx.listener(move |this, _, _, cx| {
                Online::go(this, Target::Season(id.clone(), name.clone()), cx)
            }))
            .into_any_element()
    }
}
