use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::timetrial::api::Api;
use swofty_core::timetrial::boards::{Group, PendingLap, Row};

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
    Pending,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Accept,
    Deny,
    Ban,
}

pub(crate) struct TtPage {
    pub view: View,
    pub pending: Load<Vec<PendingLap>>,
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
            view: View::Boards,
            pending: Load::Idle,
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
            self.tt_load_pending(cx);
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

    pub(super) fn tt_load_pending(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.tt_api().filter(|_| self.tt_admin()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { api.pending_laps() })
                .await;
            this.update(cx, |s, cx| {
                s.tt.pending = match got {
                    Ok(laps) => Load::Ready(laps),
                    Err(e) => Load::Failed(e.message),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn tt_select_lap(&mut self, lap: &str, cx: &mut Context<Self>) {
        self.tt.open_lap = match self.tt.open_lap.as_deref() == Some(lap) {
            true => None,
            false => Some(lap.to_owned()),
        };
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
                        s.tt.open_lap = None;
                        s.tt.banning = false;
                        s.tt_load_pending(cx);
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
