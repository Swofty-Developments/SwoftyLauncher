//! A board's track as a row: on the season page and the Time Trial page, it opens in place onto the top of its
//! board, or on the board's own page as in 1.10.

mod peek;

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

/// From this width a track's row that opens in place shows how many drivers and laps its board has.
const WITH_STATS: f32 = 760.;

/// What a track's row opens: its board in place under it when `in_place`, else the board's own page.
pub(super) fn track_item(in_place: bool, t: &TrackCard) -> Target {
    match in_place {
        true => Target::Track(t.key.clone()),
        false => Target::Board(Box::new(t.clone()), t.name.clone()),
    }
}

impl Shell {
    /// `tracks` as a panel's rows, `width` wide. With `in_place` a row opens onto the top of its board under it,
    /// else it opens the board's own page. Each row is a keyboard item.
    pub(super) fn track_rows(
        &mut self,
        tracks: &[TrackCard],
        width: f32,
        in_place: bool,
        items: &mut Vec<Target>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let bylines = self.online_bylines(&tracks.iter().collect::<Vec<_>>());
        let mut rows = Vec::new();
        for (t, byline) in tracks.iter().zip(bylines) {
            let idx = self.target_item(items, track_item(in_place, t));
            let open = in_place && self.online.peek.is_open(&t.key);
            rows.push(self.listed_track_row(t, byline, idx, in_place, open, width, p, cx));
            if open {
                rows.push(self.peek_panel(t, width, p, cx));
            }
        }
        rows
    }

    #[allow(clippy::too_many_arguments)]
    fn listed_track_row(
        &self,
        t: &TrackCard,
        byline: Option<String>,
        idx: usize,
        in_place: bool,
        open: bool,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mine = self.online.my_times.get(&t.key).copied();
        let ringed = self.online.ringed(idx);
        let row = self
            .track_row(t, byline, mine, !in_place || width >= WITH_STATS, p)
            .id(SharedString::from(format!(
                "track-{}-{}-{}",
                t.key.content, t.key.track, t.key.weather
            )))
            .border_b_1()
            .border_color(p.border_variant)
            .cursor_pointer();
        if !in_place {
            let target = track_item(false, t);
            return row
                .when(ringed, |d| d.bg(p.selected))
                .hover(move |s| s.bg(p.hover))
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size(px(14.))
                        .text_color(p.text_placeholder),
                )
                .on_click(cx.listener(move |this, _, _, cx| Online::go(this, target.clone(), cx)))
                .into_any_element();
        }
        let key = t.key.clone();
        row.when(open, |d| d.bg(p.selected))
            .when(ringed, |d| d.border_1().border_color(p.accent))
            .hover(move |s| s.bg(p.hover))
            .child(
                Icon::new(if open {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                })
                .size(px(14.))
                .text_color(if open { p.text } else { p.text_placeholder }),
            )
            .on_click(cx.listener(move |this, _, _, cx| Online::toggle_track(this, key.clone(), cx)))
            .into_any_element()
    }

    /// A track's row: its layout, name, record and holder, how busy its board is when `stats`, and this player's
    /// time.
    fn track_row(
        &self,
        t: &TrackCard,
        byline: Option<String>,
        mine: Option<u64>,
        stats: bool,
        p: Palette,
    ) -> Div {
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
            .when(stats, |d| {
                d.child(
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
            })
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
