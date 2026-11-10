use super::api::{self, Board, BoardRow, Driver, DriverBest, Season, TrackEntry, User};
use super::{BoardKey, Lens, Load};
use crate::format::{iso_date, lap_time, sector_time};
use crate::pages::{GUTTER, avatar};
use crate::scale::{Hit, px};
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use crate::ui::tokens::Tone;
use crate::ui::{Callout, EmptyState, Pill, Segmented};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf, trn};

const SECTORS_FROM: f32 = 820.;

fn cell(width: f32) -> Div {
    div().w(px(width)).flex_shrink_0()
}

fn head_cell(width: f32, text: &'static str, p: Palette) -> Div {
    cell(width)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text_placeholder)
        .child(text)
}

fn centred(p: Palette, body: impl IntoElement) -> AnyElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .pb(px(48.))
        .text_color(p.text_muted)
        .child(body)
        .into_any_element()
}

fn loading(what: &'static str, p: Palette) -> AnyElement {
    centred(
        p,
        v_flex()
            .items_center()
            .gap(px(10.))
            .child(
                Icon::new(IconName::LoaderCircle)
                    .size(px(22.))
                    .text_color(p.accent),
            )
            .child(div().text_size(px(13.)).child(what)),
    )
}

fn plural_laps(n: u32) -> String {
    trn(n as usize, "{count} lap", "{count} laps")
}

impl Shell {
    pub(crate) fn online_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        self.online_enter(cx);
        let lens = self.online.lens;
        let browsing = self.online.open.is_none() && self.online.driver.is_none();
        let actions = h_flex()
            .gap(px(8.))
            .items_center()
            .when(browsing, |d| {
                let (a, b) = (cx.entity().downgrade(), cx.entity().downgrade());
                d.child(
                    Segmented::new("online-lens", p)
                        .item(tr("Seasons"), lens == Lens::Seasons, move |_, cx| {
                            a.update(cx, |s, cx| {
                                s.online.lens = Lens::Seasons;
                                cx.notify();
                            })
                            .ok();
                        })
                        .item(tr("Tracks"), lens == Lens::Tracks, move |_, cx| {
                            b.update(cx, |s, cx| {
                                s.online.lens = Lens::Tracks;
                                cx.notify();
                            })
                            .ok();
                        }),
                )
            })
            .child(
                Button::new("online-refresh")
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(tr("Refresh"))
                    .on_click(cx.listener(|this, _, _, cx| this.online_reload(cx))),
            );
        let signed_out = self.mp.is_none().then(|| {
            div().px(px(GUTTER)).pb(px(12.)).child(
                Callout::info(tr(
                    "Log in with Discord to see your own laps marked on the boards.",
                ))
                .icon(IconName::User)
                .actions(
                    h_flex().child(
                        Button::new("online-login")
                            .outline()
                            .tap_sm()
                            .label(tr("Log in with Discord"))
                            .on_click(cx.listener(|this, _, window, cx| this.mp_sign_in(window, cx))),
                    ),
                ),
            )
        });
        let body = if self.online.driver.is_some() {
            self.driver_view(p, cx)
        } else if self.online.open.is_some() {
            self.board_view(p, cx)
        } else {
            match lens {
                Lens::Seasons => self.seasons_view(p, cx),
                Lens::Tracks => self.tracks_view(p, cx),
            }
        };
        v_flex()
            .size_full()
            .child(self.header(
                tr("Online"),
                tr("Time Trial leaderboards, one board per track and season.").into(),
                actions,
                p,
            ))
            .children(signed_out)
            .child(v_flex().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn failed(&self, e: &api::OnlineError, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let _ = p;
        EmptyState::new(IconName::CloudOff, tr("Can't load the boards"))
            .body(e.message.clone())
            .action(
                Button::new("online-retry")
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(tr("Try again"))
                    .on_click(cx.listener(|this, _, _, cx| this.online_reload(cx))),
            )
            .into_any_element()
    }

    fn nothing_yet(&self) -> AnyElement {
        EmptyState::new(IconName::Trophy, tr("No laps yet"))
            .body(tr("Time Trial laps show up here once they are uploaded."))
            .into_any_element()
    }

    fn seasons_view(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.online.seasons {
            Load::Idle | Load::Loading => loading(tr("Loading the boards…"), p),
            Load::Failed(e) => {
                let e = e.clone();
                self.failed(&e, p, cx)
            }
            Load::Ready(seasons) if seasons.iter().all(|s| s.tracks.is_empty()) => self.nothing_yet(),
            Load::Ready(seasons) => {
                let blocks: Vec<AnyElement> = seasons
                    .iter()
                    .filter(|s| !s.tracks.is_empty())
                    .map(|s| self.season_block(s, p, cx))
                    .collect();
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("online-seasons")
                    .px(px(GUTTER))
                    .pb(px(24.))
                    .gap(px(18.))
                    .children(blocks)
                    .into_any_element()
            }
        }
    }

    fn season_block(&self, season: &Season, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = season
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let key = BoardKey {
                    content: season.content.clone(),
                    track: t.track.clone(),
                    weather: t.weather.clone(),
                };
                h_flex()
                    .id(SharedString::from(format!(
                        "season-{}-{}-{}",
                        season.content, t.track, t.weather
                    )))
                    .h(px(44.))
                    .px(px(14.))
                    .gap(px(12.))
                    .items_center()
                    .cursor_pointer()
                    .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
                    .hover(move |s| s.bg(p.hover))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(8.))
                            .child(div().truncate().font_weight(FontWeight::MEDIUM).child(t.title()))
                            .when(t.custom, |d| d.child(Pill::new(tr("Custom track"))))
                            .child(Pill::new(t.weather.clone())),
                    )
                    .child(
                        cell(100.)
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .child(plural_laps(t.laps)),
                    )
                    .child(cell(110.).text_size(px(13.)).text_color(p.text_muted).child(trn(
                        t.players as usize,
                        "{count} driver",
                        "{count} drivers",
                    )))
                    .child(
                        cell(90.)
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t.best_ms.map(lap_time).unwrap_or_default()),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(14.))
                            .text_color(p.text_placeholder),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.online_open_board(key.clone(), cx)))
                    .into_any_element()
            })
            .collect();
        v_flex()
            .flex_shrink_0()
            .gap(px(8.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(season.name.clone()),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_placeholder).child(trn(
                        rows.len(),
                        "{count} track",
                        "{count} tracks",
                    ))),
            )
            .child(
                v_flex()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(p.border)
                    .overflow_hidden()
                    .children(rows),
            )
            .into_any_element()
    }

    fn tracks_view(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.online.tracks {
            Load::Idle | Load::Loading => loading(tr("Loading the boards…"), p),
            Load::Failed(e) => {
                let e = e.clone();
                self.failed(&e, p, cx)
            }
            Load::Ready(tracks) if tracks.is_empty() => self.nothing_yet(),
            Load::Ready(tracks) => {
                let blocks: Vec<AnyElement> = tracks.iter().map(|t| self.track_block(t, p, cx)).collect();
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("online-tracks")
                    .px(px(GUTTER))
                    .pb(px(24.))
                    .gap(px(18.))
                    .children(blocks)
                    .into_any_element()
            }
        }
    }

    fn track_block(&self, track: &TrackEntry, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let cards: Vec<AnyElement> = track
            .boards
            .iter()
            .map(|b| {
                let key = BoardKey {
                    content: b.content.clone(),
                    track: track.track.clone(),
                    weather: b.weather.clone(),
                };
                v_flex()
                    .id(SharedString::from(format!(
                        "track-{}-{}-{}",
                        track.track, b.content, b.weather
                    )))
                    .w(px(250.))
                    .flex_shrink_0()
                    .gap(px(4.))
                    .p(px(12.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(p.border)
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(b.name.clone()),
                            )
                            .child(Pill::new(b.weather.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(b.best_ms.map(lap_time).unwrap_or_else(|| "-".into())),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .truncate()
                            .child(match &b.best_user {
                                Some(who) => {
                                    trf!("{laps} - best by {who}", laps = plural_laps(b.laps), who = who)
                                }
                                None => plural_laps(b.laps),
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.online_open_board(key.clone(), cx)))
                    .into_any_element()
            })
            .collect();
        v_flex()
            .flex_shrink_0()
            .gap(px(8.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(track.title()),
                    )
                    .when(track.custom, |d| d.child(Pill::new(tr("Custom track")))),
            )
            .child(h_flex().flex_wrap().gap(px(10.)).children(cards))
            .into_any_element()
    }

    fn online_back(
        &self,
        title: String,
        sub: String,
        id: &'static str,
        back: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .px(px(GUTTER))
            .pb(px(12.))
            .gap(px(12.))
            .items_center()
            .child(
                Button::new(id)
                    .outline()
                    .tap_sm()
                    .icon(IconName::ChevronLeft)
                    .label(tr("Back"))
                    .on_click(cx.listener(move |this, _, _, cx| back(this, cx))),
            )
            .child(
                v_flex()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_muted).child(sub)),
            )
            .into_any_element()
    }

    fn board_view(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let key = self.online.open.clone().expect("a board is open");
        match &self.online.board {
            Load::Idle | Load::Loading => v_flex()
                .size_full()
                .child(self.online_back(
                    api::track_title(&key.track, None),
                    key.weather.clone(),
                    "board-back",
                    |s, cx| s.online_close_board(cx),
                    p,
                    cx,
                ))
                .child(loading(tr("Loading the board…"), p))
                .into_any_element(),
            Load::Failed(e) => {
                let e = e.clone();
                v_flex()
                    .size_full()
                    .child(self.online_back(
                        api::track_title(&key.track, None),
                        key.weather.clone(),
                        "board-back",
                        |s, cx| s.online_close_board(cx),
                        p,
                        cx,
                    ))
                    .child(self.failed(&e, p, cx))
                    .into_any_element()
            }
            Load::Ready(board) => {
                let board = board.clone();
                self.board_table(&board, p, cx)
            }
        }
    }

    fn board_table(&mut self, board: &Board, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let sub = format!(
            "{} - {} - {}",
            board.name,
            board.weather,
            plural_laps(board.total)
        );
        let bar = self.online_back(
            board.title(),
            sub,
            "board-back",
            |s, cx| s.online_close_board(cx),
            p,
            cx,
        );
        if board.rows.is_empty() {
            return v_flex()
                .size_full()
                .child(bar)
                .child(self.nothing_yet())
                .into_any_element();
        }
        let sectors = self.content_width >= SECTORS_FROM;
        let me = self.mp.as_ref().map(|m| m.name.clone());
        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, row) in board.rows.iter().enumerate() {
            let mine = me.as_deref() == Some(row.user.name.as_str());
            rows.push(self.board_row(row, i, mine, sectors, p, cx));
            if self.online.viewing.as_deref() == Some(row.lap_id.as_str()) {
                rows.push(self.lap_actions(&row.lap_id, p, cx));
            }
        }
        let remaining = board.total.saturating_sub(board.rows.len() as u32);
        let more = (remaining > 0).then(|| {
            Button::new("board-more")
                .outline()
                .disabled(self.online.more)
                .label(if self.online.more {
                    tr("Loading…")
                } else {
                    tr("Show more laps")
                })
                .on_click(cx.listener(|this, _, _, cx| this.online_more(cx)))
        });
        v_flex()
            .size_full()
            .child(bar)
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("online-board")
                    .px(px(GUTTER))
                    .pb(px(24.))
                    .gap(px(12.))
                    .child(
                        v_flex()
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(p.border)
                            .overflow_hidden()
                            .child(
                                h_flex()
                                    .h(px(30.))
                                    .px(px(14.))
                                    .gap(px(12.))
                                    .items_center()
                                    .bg(p.surface)
                                    .child(head_cell(44., tr("RANK"), p))
                                    .child(div().flex_1().min_w_0().child(head_cell(120., tr("DRIVER"), p)))
                                    .child(head_cell(90., tr("TIME"), p))
                                    .when(sectors, |d| {
                                        d.child(head_cell(70., tr("S1"), p))
                                            .child(head_cell(70., tr("S2"), p))
                                            .child(head_cell(70., tr("S3"), p))
                                    })
                                    .child(head_cell(88., tr("DATE"), p))
                                    .child(cell(76.)),
                            )
                            .children(rows),
                    )
                    .children(more),
            )
            .into_any_element()
    }

    fn board_row(
        &self,
        row: &BoardRow,
        i: usize,
        mine: bool,
        sectors: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let user = row.user.clone();
        let lap = row.lap_id.clone();
        let open = self.online.viewing.as_deref() == Some(row.lap_id.as_str());
        let podium = match row.rank {
            1 => Some(p.warning),
            _ => None,
        };
        h_flex()
            .h(px(42.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border_variant)
            .when(mine, |d| d.bg(p.selected))
            .child(
                cell(44.)
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(podium.unwrap_or(p.text_muted))
                    .child(row.rank.to_string()),
            )
            .child(
                h_flex()
                    .id(SharedString::from(format!("driver-{i}-{}", row.lap_id)))
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.))
                    .items_center()
                    .cursor_pointer()
                    .child(avatar(&user.name, 24., p).rounded_full())
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(user.name.clone()),
                    )
                    .when(mine, |d| d.child(Pill::new(tr("You")).tone(Tone::Info)))
                    .when(row.status == "unranked", |d| d.child(Pill::new(tr("Unranked"))))
                    .hover(move |s| s.text_color(p.accent_hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.online_open_driver(user.clone(), cx))),
            )
            .child(
                cell(90.)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(lap_time(row.time_ms)),
            )
            .when(sectors, |d| {
                d.children((0..3).map(|n| {
                    cell(70.).text_size(px(13.)).text_color(p.text_muted).child(
                        row.sectors_ms
                            .get(n)
                            .map(|s| sector_time(*s))
                            .unwrap_or_else(|| "-".into()),
                    )
                }))
            })
            .child(
                cell(88.)
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(iso_date(&row.set_at).to_owned()),
            )
            .child(
                cell(76.).child(
                    Button::new(SharedString::from(format!("view-{}", row.lap_id)))
                        .outline()
                        .tap_xs()
                        .label(tr("View"))
                        .when(open, |b| b.primary())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let same = this.online.viewing.as_deref() == Some(lap.as_str());
                            this.online.viewing = (!same).then(|| lap.clone());
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }

    fn lap_actions(&self, lap_id: &str, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let url = api::lap_url(lap_id);
        let running = self.online_game_running();
        let ready = self.online.replays.available();
        let id = lap_id.to_owned();
        h_flex()
            .px(px(14.))
            .py(px(8.))
            .gap(px(8.))
            .items_center()
            .bg(p.surface)
            .border_t_1()
            .border_color(p.border_variant)
            .child(
                Button::new(SharedString::from(format!("web-{lap_id}")))
                    .outline()
                    .tap_sm()
                    .icon(IconName::ExternalLink)
                    .label(tr("Open on the web"))
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .when(running, |d| {
                d.child(
                    Button::new(SharedString::from(format!("watch-{lap_id}")))
                        .outline()
                        .tap_sm()
                        .icon(IconName::Play)
                        .label(tr("Watch in game"))
                        .disabled(!ready)
                        .tooltip(if ready {
                            tr("Play this lap's replay in the running game")
                        } else {
                            tr("In-game replays are not available yet")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Err(e) = this.online.replays.watch(&id) {
                                crate::app_log::action("watch a lap in game").failed(&e);
                            }
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn driver_view(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some((user, state)) = &self.online.driver else {
            return div().into_any_element();
        };
        let user: User = user.clone();
        let bar = self.online_back(
            user.name.clone(),
            tr("The driver's best lap on every board").to_owned(),
            "driver-back",
            |s, cx| s.online_close_driver(cx),
            p,
            cx,
        );
        let body = match state {
            Load::Idle | Load::Loading => loading(tr("Loading the driver's laps…"), p),
            Load::Failed(e) => {
                let e = e.clone();
                self.failed(&e, p, cx)
            }
            Load::Ready(driver) if driver.bests.is_empty() => self.nothing_yet(),
            Load::Ready(driver) => {
                let driver: Driver = driver.clone();
                let mut rows: Vec<AnyElement> = Vec::new();
                for (i, best) in driver.bests.iter().enumerate() {
                    rows.push(self.best_row(best, i, p, cx));
                    if self.online.viewing.as_deref() == Some(best.lap_id.as_str()) {
                        rows.push(self.lap_actions(&best.lap_id, p, cx));
                    }
                }
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("online-driver")
                    .px(px(GUTTER))
                    .pb(px(24.))
                    .child(
                        v_flex()
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(p.border)
                            .overflow_hidden()
                            .children(rows),
                    )
                    .into_any_element()
            }
        };
        v_flex().size_full().child(bar).child(body).into_any_element()
    }

    fn best_row(&self, best: &DriverBest, i: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let lap = best.lap_id.clone();
        let open = self.online.viewing.as_deref() == Some(best.lap_id.as_str());
        h_flex()
            .h(px(44.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(best.title()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(format!("{} - {}", best.name, best.weather)),
                    ),
            )
            .child(
                cell(60.)
                    .text_color(p.text_muted)
                    .child(best.rank.map(|r| format!("#{r}")).unwrap_or_default()),
            )
            .child(
                cell(90.)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(lap_time(best.time_ms)),
            )
            .child(
                cell(88.)
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(iso_date(&best.set_at).to_owned()),
            )
            .child(
                cell(76.).child(
                    Button::new(SharedString::from(format!("view-best-{}", best.lap_id)))
                        .outline()
                        .tap_xs()
                        .label(tr("View"))
                        .when(open, |b| b.primary())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let same = this.online.viewing.as_deref() == Some(lap.as_str());
                            this.online.viewing = (!same).then(|| lap.clone());
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
}
