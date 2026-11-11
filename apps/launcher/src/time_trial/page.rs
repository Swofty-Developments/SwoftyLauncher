use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::timetrial::boards::{Row, format_time};
use swofty_i18n::{tr, trf};

use super::moderation::flag;
use super::state::{Load, Pick, View};
use crate::pages::GUTTER;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use crate::ui::{EmptyState, selectable_row};

const LIST_WIDTH: f32 = 270.;

impl Shell {
    pub(crate) fn time_trial_page(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refresh = Button::new("tt-refresh")
            .outline()
            .icon(IconName::ArrowRightLeft)
            .label(tr("Refresh"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.tt_load_catalog(cx);
                this.tt_reload(cx);
                this.tt_load_runs(cx);
            }));
        if self.mp.is_none() {
            let header = self.header(
                tr("Time Trial"),
                tr("Leaderboards for every track").to_owned(),
                refresh,
                p,
            );
            return v_flex()
                .size_full()
                .child(header)
                .child(
                    EmptyState::new(IconName::Trophy, tr("Sign in to see the Time Trial boards"))
                        .body(tr("Sign in from Settings with Discord.")),
                )
                .into_any_element();
        }
        let admin = self.tt_admin();
        let pending_view = admin && self.tt.view == View::Runs;
        let body = match pending_view {
            true => self.tt_runs(p, window, cx),
            false => {
                let tracks = self.tt_tracks(p, cx);
                let board = self.tt_board(p, window, cx);
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(16.))
                    .px(px(GUTTER))
                    .pb(px(16.))
                    .child(tracks)
                    .child(board)
                    .into_any_element()
            }
        };
        let tabs = admin.then(|| self.tt_tabs(p, cx));
        let header = self.header(
            tr("Time Trial"),
            tr("Leaderboards for every track").to_owned(),
            refresh,
            p,
        );
        v_flex()
            .size_full()
            .child(header)
            .children(tabs)
            .child(body)
            .into_any_element()
    }

    fn tt_tracks(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let frame = || {
            v_flex()
                .w(px(LIST_WIDTH))
                .flex_shrink_0()
                .h_full()
                .min_h_0()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
        };
        let note = |text: String| {
            frame()
                .p(px(12.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(text)
                .into_any_element()
        };
        let groups = match &self.tt.catalog {
            Load::Idle | Load::Loading => return note(tr("Loading tracks…").to_owned()),
            Load::Failed(e) => return note(e.clone()),
            Load::Ready(g) if g.is_empty() => return note(tr("No Time Trial boards yet.").to_owned()),
            Load::Ready(g) => g,
        };
        let selected = self.tt.pick.clone();
        let mut list = v_flex().gap(px(2.)).p(px(6.));
        for (gi, group) in groups.iter().enumerate() {
            let heading = match group.name.is_empty() {
                true => group.content.clone(),
                false => group.name.clone(),
            };
            list = list.child(
                div()
                    .px(px(8.))
                    .pt(px(if gi == 0 { 2. } else { 10. }))
                    .pb(px(2.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text_placeholder)
                    .child(heading.to_uppercase()),
            );
            for (ti, track) in group.tracks.iter().enumerate() {
                let pick = Pick {
                    content: group.content.clone(),
                    track: track.track.clone(),
                    weather: track.weather.clone(),
                    title: track.title(),
                };
                let active = selected.as_ref() == Some(&pick);
                let weather = (track.weather != "dry").then(|| track.weather.clone());
                list = list.child(
                    selectable_row(SharedString::from(format!("tt-track-{gi}-{ti}")), active, p)
                        .cursor_pointer()
                        .when(!active, |d| d.hover(move |s| s.bg(p.hover)))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_size(px(13.)).text_color(p.text).child(track.title()))
                                .child(div().text_size(px(11.)).text_color(p.text_muted).child(
                                    match &weather {
                                        Some(w) => format!("{w} · {}", laps(track.laps)),
                                        None => laps(track.laps),
                                    },
                                )),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.tt_pick(pick.clone(), cx))),
                );
            }
        }
        frame()
            .child(div().flex_1().min_h_0().scroll_y("tt-tracks").child(list))
            .into_any_element()
    }

    fn tt_board(&mut self, p: Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(pick) = self.tt.pick.clone() else {
            return div()
                .flex_1()
                .child(EmptyState::new(
                    IconName::Timer,
                    tr("Pick a track to see its leaderboard"),
                ))
                .into_any_element();
        };
        let search = self.tt_search(window, cx);
        let rows: Vec<Row> = self.tt.visible().into_iter().cloned().collect();
        let rows_len = rows.len();
        let total = match &self.tt.rows {
            Load::Ready(r) => r.len(),
            _ => 0,
        };
        let body: AnyElement = match &self.tt.rows {
            Load::Idle | Load::Loading => note(tr("Loading the leaderboard…"), p),
            Load::Failed(e) => note(e, p),
            Load::Ready(_) if total == 0 => note(tr("No laps on this board yet."), p),
            Load::Ready(_) if rows.is_empty() => note(tr("No driver matches that name."), p),
            Load::Ready(_) => {
                let mut built = Vec::new();
                for (i, r) in rows.iter().enumerate() {
                    built.push(self.tt_row(i, r, p, window, cx));
                }
                let rows = built;
                div()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("tt-rows")
                    .child(v_flex().children(rows))
                    .into_any_element()
            }
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .gap(px(10.))
            .child(
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(pick.title.clone()),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_muted).child(trf!(
                        "{shown} of {total} drivers",
                        shown = rows_len,
                        total = total
                    )))
                    .child(
                        div().w(px(240.)).child(
                            Input::new(&search).small().cleanable(true).prefix(
                                Icon::new(IconName::Search)
                                    .size(px(14.))
                                    .text_color(p.text_placeholder),
                            ),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(p.border)
                    .overflow_hidden()
                    .child(body),
            )
            .into_any_element()
    }

    fn tt_row(
        &mut self,
        i: usize,
        row: &Row,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let place = row.rank.map(|r| r as usize).unwrap_or(i + 1);
        let flagged = row.pending() && self.tt_admin();
        let open = flagged && self.tt.open_lap.as_deref() == Some(row.lap_id.as_str());
        let id = row.lap_id.clone();
        let line = h_flex()
            .id(SharedString::from(format!("tt-row-{}", row.lap_id)))
            .h(px(40.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .when(flagged, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.tt_select_lap(&id, cx)))
            })
            .when(open, |d| d.bg(p.surface))
            .child(
                div()
                    .w(px(32.))
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(match row.pending() {
                        true => "-".to_owned(),
                        false => place.to_string(),
                    }),
            )
            .when(flagged, |d| d.child(flag(p)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(14.))
                    .text_color(p.text)
                    .child(row.driver.name.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(format_time(row.time_ms)),
            )
            .child(
                div()
                    .w(px(96.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(row.date.chars().take(10).collect::<String>()),
            );
        let panel = open.then(|| {
            self.tt_actions(
                &row.lap_id,
                &row.reasons,
                row.game_replay.is_some(),
                p,
                window,
                cx,
            )
        });
        v_flex()
            .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
            .child(line)
            .children(panel)
            .into_any_element()
    }
}

fn laps(n: u32) -> String {
    swofty_i18n::trn(n as usize, "{count} lap", "{count} laps")
}

fn note(text: impl Into<SharedString>, p: Palette) -> AnyElement {
    div()
        .p(px(14.))
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(text.into())
        .into_any_element()
}
