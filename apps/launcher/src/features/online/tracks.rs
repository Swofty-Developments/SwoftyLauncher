//! A board's track as a row on the season page, opening the board's own page.

use super::model::*;
use super::{Online, Target};
use crate::shell::Shell;
use crate::ui::format::lap_time;
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trn};

impl Shell {
    /// `tracks` as a panel's rows, each opening its board's own page and each a keyboard item.
    pub(super) fn track_rows(
        &mut self,
        tracks: &[TrackCard],
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let bylines = self.online_bylines(&tracks.iter().collect::<Vec<_>>());
        let mut rows = Vec::new();
        for (t, byline) in tracks.iter().zip(bylines) {
            let target = Target::Board(Box::new(t.clone()), t.name.clone());
            let idx = self.target_item(items, target.clone());
            rows.push(self.listed_track_row(t, byline, idx, target, p, cx));
        }
        rows
    }

    fn listed_track_row(
        &self,
        t: &TrackCard,
        byline: Option<String>,
        idx: usize,
        target: Target,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mine = self.online.my_times.get(&t.key).copied();
        self.track_row(t, byline, mine, p)
            .id(SharedString::from(format!(
                "track-{}-{}-{}",
                t.key.content, t.key.track, t.key.weather
            )))
            .border_b_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .when(self.online.ringed(idx), |d| d.bg(p.selected))
            .hover(move |s| s.bg(p.hover))
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(p.text_placeholder),
            )
            .on_click(cx.listener(move |this, _, _, cx| Online::go(this, target.clone(), cx)))
            .into_any_element()
    }

    /// A track's row: its layout, name, record and holder, how busy its board is, and this player's time.
    fn track_row(&self, t: &TrackCard, byline: Option<String>, mine: Option<u64>, p: Palette) -> Div {
        h_flex()
            .h(px(42.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .child(self.layout_art(t.layout.as_ref(), 28., p.accent_hover, p))
            .child(
                v_flex()
                    .w(px(200.))
                    .flex_shrink_0()
                    .min_w_0()
                    .child(
                        h_flex()
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
                    .children(byline.map(|b| {
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(p.text_muted)
                            .child(b)
                    })),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .gap(px(8.))
                    .items_center()
                    .children(t.record.as_ref().map(|r| {
                        h_flex()
                            .min_w_0()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .flex_shrink_0()
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
                                .child(tr("No time yet")),
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
    }
}
