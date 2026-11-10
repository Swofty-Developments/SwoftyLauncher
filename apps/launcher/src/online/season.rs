use super::model::*;
use super::widgets::*;
use super::{Load, Target};
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trn};

impl Shell {
    pub(crate) fn online_season(&mut self, id: &str, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let page = match self.online.seasons.get(id) {
            None | Some(Load::Idle | Load::Loading) => return self.skeleton(p, 5, 110.),
            Some(Load::Failed(e)) => {
                let e = e.clone();
                return self.failed_view(&e, cx);
            }
            Some(Load::Ready(page)) => page.clone(),
        };
        if page.tracks.is_empty() {
            return self.nothing_yet();
        }
        let mut items: Vec<Target> = Vec::new();
        let me = self.account_name();
        let wide = self.content_width >= 980.;
        let rows: Vec<AnyElement> = page
            .tracks
            .iter()
            .map(|t| {
                let idx = self.target_item(&mut items, Target::Board(Box::new(t.clone()), t.name.clone()));
                self.track_row(t, idx, p, cx)
            })
            .collect();
        let ranking: Vec<AnyElement> = page
            .ranking
            .iter()
            .map(|r| {
                let idx = self.target_item(&mut items, Target::Profile(r.driver.clone()));
                self.rank_row(r, idx, me.as_deref() == Some(r.driver.name.as_str()), p, cx)
            })
            .collect();
        self.online.items = items;
        let header = self.season_header(&page.season, p);
        let ranking_panel = panel(p)
            .w(px(if wide { 330. } else { 0. }))
            .flex_shrink_0()
            .child(
                div()
                    .px(px(14.))
                    .pt(px(14.))
                    .pb(px(8.))
                    .child(section_title(tr("SEASON RANKING"), p)),
            )
            .children(ranking);
        v_flex()
            .flex_1()
            .min_h_0()
            .scroll_y("online-season")
            .px(px(28.))
            .pb(px(28.))
            .gap(px(18.))
            .child(header)
            .child(
                h_flex()
                    .gap(px(18.))
                    .items_start()
                    .child(panel(p).flex_1().min_w_0().children(rows))
                    .when(wide, |d| d.child(ranking_panel)),
            )
            .into_any_element()
    }

    fn season_header(&self, s: &SeasonCard, p: Palette) -> AnyElement {
        div()
            .relative()
            .h(px(150.))
            .rounded(px(14.))
            .overflow_hidden()
            .child(self.tt_cover(s.cover.as_ref(), &s.id))
            .child(div().absolute().inset_0().bg(linear_gradient(
                90.,
                linear_color_stop(hsla(0., 0., 0., 0.8), 0.),
                linear_color_stop(hsla(0., 0., 0., 0.15), 1.),
            )))
            .child(
                v_flex()
                    .absolute()
                    .left(px(24.))
                    .bottom(px(20.))
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(30.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child(s.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(12.))
                            .items_center()
                            .child(div().text_size(px(13.)).text_color(hsla(0., 0., 1., 0.85)).child(
                                format!(
                                    "{} - {}",
                                    trn(s.tracks as usize, "{count} track", "{count} tracks"),
                                    trn(s.drivers as usize, "{count} driver", "{count} drivers")
                                ),
                            ))
                            .child(self.tt_faces(&s.top, 28., p)),
                    ),
            )
            .into_any_element()
    }
}
