use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::timetrial::api::Api;
use swofty_core::timetrial::boards::{Group, Row, Run, RunBoard};

use crate::shell::Shell;

pub(crate) enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Pick {
    pub content: String,
    pub track: String,
    pub weather: String,
    pub title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Boards,
    Runs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Accept,
    Deny,
    Ban,
}

pub(crate) struct TtPage {
    pub view: View,
    pub runs: Load<Vec<Run>>,
    pub run_boards: Vec<RunBoard>,
    pub run_seasons: Vec<String>,
    pub run_tracks: Vec<String>,
    pub run_status: &'static str,
    pub run_query: String,
    pub run_search: Option<Entity<InputState>>,
    run_sub: Option<Subscription>,
    run_generation: u64,
    pub open_lap: Option<String>,
    pub banning: bool,
    pub acting: bool,
    pub notice: Option<String>,
    pub reason: Option<Entity<InputState>>,
    pub catalog: Load<Vec<Group>>,
    pub pick: Option<Pick>,
    pub rows: Load<Vec<Row>>,
    pub query: String,
    pub search: Option<Entity<InputState>>,
    sub: Option<Subscription>,
    generation: u64,
}

impl Default for TtPage {
    fn default() -> Self {
        Self {
            view: View::Runs,
            runs: Load::Idle,
            run_boards: Vec::new(),
            run_seasons: Vec::new(),
            run_tracks: Vec::new(),
            run_status: "all",
            run_query: String::new(),
            run_search: None,
            run_sub: None,
            run_generation: 0,
            open_lap: None,
            banning: false,
            acting: false,
            notice: None,
            reason: None,
            catalog: Load::Idle,
            pick: None,
            rows: Load::Idle,
            query: String::new(),
            search: None,
            sub: None,
            generation: 0,
        }
    }
}

impl TtPage {
    /// The loaded runs that pass the status choice and the search text, newest first as loaded.
    pub(super) fn shown_runs(&self) -> Vec<&Run> {
        let Load::Ready(runs) = &self.runs else {
            return Vec::new();
        };
        runs.iter()
            .filter(|r| self.run_status == "all" || r.status == self.run_status)
            .filter(|r| r.matches(&self.run_query))
            .collect()
    }

    pub(super) fn waiting(&self) -> usize {
        match &self.runs {
            Load::Ready(runs) => runs.iter().filter(|r| r.pending()).count(),
            _ => 0,
        }
    }

    pub(super) fn visible(&self) -> Vec<&Row> {
        let Load::Ready(rows) = &self.rows else {
            return Vec::new();
        };
        swofty_core::timetrial::boards::by_driver(rows, &self.query)
    }
}

impl Shell {
    pub(super) fn tt_api(&self) -> Option<Api> {
        self.mp.as_ref().map(|s| Api::new(&s.token))
    }

    pub(crate) fn tt_admin(&self) -> bool {
        self.mp.as_ref().is_some_and(|s| s.admin)
    }

    pub(crate) fn tt_open(&mut self, cx: &mut Context<Self>) {
        if self.tt_admin() {
            self.tt_load_runs(cx);
        } else {
            self.tt.view = View::Boards;
        }
        if matches!(self.tt.catalog, Load::Idle | Load::Failed(_)) {
            self.tt_load_catalog(cx);
        }
    }

    pub(super) fn tt_load_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.tt_api() else {
            return;
        };
        self.tt.catalog = Load::Loading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx.background_executor().spawn(async move { api.catalog() }).await;
            this.update(cx, |s, cx| {
                s.tt.catalog = match got {
                    Ok(groups) => Load::Ready(groups),
                    Err(e) => Load::Failed(e.message),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn tt_load_runs(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.tt_api().filter(|_| self.tt_admin()) else {
            return;
        };
        self.tt.run_generation += 1;
        let generation = self.tt.run_generation;
        let (seasons, tracks) = (self.tt.run_seasons.clone(), self.tt.run_tracks.clone());
        if !matches!(self.tt.runs, Load::Ready(_)) {
            self.tt.runs = Load::Loading;
        }
        cx.spawn(async move |this, cx| {
            let (runs, boards) = cx
                .background_executor()
                .spawn(async move { (api.runs(&seasons, &tracks), api.run_boards()) })
                .await;
            this.update(cx, |s, cx| {
                if s.tt.run_generation != generation {
                    return;
                }
                s.tt.runs = match runs {
                    Ok(laps) => {
                        if s.tt.open_lap.is_none() {
                            s.tt.open_lap = laps.first().map(|r| r.lap_id.clone());
                        }
                        Load::Ready(laps)
                    }
                    Err(e) => Load::Failed(e.message),
                };
                if let Ok(boards) = boards {
                    s.tt.run_boards = boards;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Turn one season or track filter on or off and load the runs again.
    pub(super) fn tt_toggle_filter(&mut self, season: bool, value: &str, cx: &mut Context<Self>) {
        let list = if season {
            &mut self.tt.run_seasons
        } else {
            &mut self.tt.run_tracks
        };
        match list.iter().position(|v| v == value) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(value.to_owned()),
        }
        self.tt_load_runs(cx);
        cx.notify();
    }

    pub(super) fn tt_run_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(state) = &self.tt.run_search {
            return state.clone();
        }
        let state = cx.new(|cx| {
            InputState::new(window, cx).placeholder(swofty_i18n::tr("Search driver, season, track or lap id"))
        });
        self.tt.run_sub = Some(cx.subscribe(&state, |this, state, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.tt.run_query = state.read(cx).value().to_string();
                cx.notify();
            }
        }));
        self.tt.run_search = Some(state.clone());
        state
    }

    pub(super) fn tt_select_lap(&mut self, lap: &str, cx: &mut Context<Self>) {
        self.tt.open_lap = Some(lap.to_owned());
        self.tt.banning = false;
        self.tt.notice = None;
        cx.notify();
    }

    pub(super) fn tt_moderate(
        &mut self,
        lap: String,
        verdict: Verdict,
        reason: String,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.tt_api().filter(|_| self.tt_admin() && !self.tt.acting) else {
            return;
        };
        self.tt.acting = true;
        self.tt.notice = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    match verdict {
                        Verdict::Accept => api.accept_lap(&lap),
                        Verdict::Deny => api.deny_lap(&lap),
                        Verdict::Ban => api.ban_driver(&lap, &reason),
                    }
                })
                .await;
            this.update(cx, |s, cx| {
                s.tt.acting = false;
                match done {
                    Ok(()) => {
                        s.tt.banning = false;
                        s.tt_load_runs(cx);
                        s.tt_reload(cx);
                    }
                    Err(e) => s.tt.notice = Some(e.message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn tt_reason(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(state) = &self.tt.reason {
            return state.clone();
        }
        let state =
            cx.new(|cx| InputState::new(window, cx).placeholder(swofty_i18n::tr("Reason for the ban")));
        cx.subscribe(&state, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        self.tt.reason = Some(state.clone());
        state
    }

    pub(super) fn tt_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        self.tt.pick = Some(pick);
        self.tt_reload(cx);
    }

    pub(super) fn tt_reload(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(pick)) = (self.tt_api(), self.tt.pick.clone()) else {
            return;
        };
        self.tt.generation += 1;
        let generation = self.tt.generation;
        self.tt.rows = Load::Loading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { api.board(&pick.content, &pick.track, &pick.weather, None) })
                .await;
            this.update(cx, |s, cx| {
                if s.tt.generation != generation {
                    return;
                }
                s.tt.rows = match got {
                    Ok(rows) => Load::Ready(rows),
                    Err(e) => Load::Failed(e.message),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn tt_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(state) = &self.tt.search {
            return state.clone();
        }
        let state = cx.new(|cx| InputState::new(window, cx).placeholder(swofty_i18n::tr("Search drivers")));
        self.tt.sub = Some(cx.subscribe(&state, |this, state, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.tt.query = state.read(cx).value().to_string();
                cx.notify();
            }
        }));
        self.tt.search = Some(state.clone());
        state
    }
}
