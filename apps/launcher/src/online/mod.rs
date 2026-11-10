mod api;
mod mock;
mod replay;
mod view;

use crate::shell::Shell;
use api::{Board, BoardsApi, Driver, HttpBoards, OnlineError, PAGE_SIZE, Season, TrackEntry, User};
use gpui_kit::*;
use replay::{NotYet, Replays};
use std::sync::Arc;

pub(crate) enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(OnlineError),
}

impl<T> Load<T> {
    fn from(result: Result<T, OnlineError>) -> Self {
        match result {
            Ok(v) => Self::Ready(v),
            Err(e) => Self::Failed(e),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lens {
    Seasons,
    Tracks,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BoardKey {
    pub content: String,
    pub track: String,
    pub weather: String,
}

pub(crate) struct Online {
    api: Arc<dyn BoardsApi>,
    replays: Box<dyn Replays>,
    pretend_running: bool,
    lens: Lens,
    seasons: Load<Vec<Season>>,
    tracks: Load<Vec<TrackEntry>>,
    open: Option<BoardKey>,
    board: Load<Board>,
    more: bool,
    driver: Option<(User, Load<Driver>)>,
    viewing: Option<String>,
    epoch: u64,
}

impl Online {
    pub(crate) fn new() -> Self {
        let (api, pretend_running): (Arc<dyn BoardsApi>, bool) = match mock_mode() {
            Some(mode) => (Arc::new(mock::MockBoards { mode }), true),
            None => (Arc::new(HttpBoards::new()), false),
        };
        Self::with(api, pretend_running)
    }

    fn with(api: Arc<dyn BoardsApi>, pretend_running: bool) -> Self {
        Self {
            api,
            replays: Box::new(NotYet),
            pretend_running,
            lens: Lens::Seasons,
            seasons: Load::Idle,
            tracks: Load::Idle,
            open: None,
            board: Load::Idle,
            more: false,
            driver: None,
            viewing: None,
            epoch: 0,
        }
    }
}

fn mock_mode() -> Option<mock::Mode> {
    if cfg!(debug_assertions) {
        mock::Mode::from_env()
    } else {
        None
    }
}

impl Shell {
    pub(crate) fn online_enter(&mut self, cx: &mut Context<Self>) {
        if matches!(self.online.seasons, Load::Idle) {
            self.online_reload(cx);
        }
    }

    pub(crate) fn online_reload(&mut self, cx: &mut Context<Self>) {
        self.online.epoch += 1;
        let epoch = self.online.epoch;
        self.online.seasons = Load::Loading;
        self.online.tracks = Load::Loading;
        let (a, b) = (self.online.api.clone(), self.online.api.clone());
        cx.spawn(async move |this, cx| {
            let seasons = cx.background_executor().spawn(async move { a.seasons() }).await;
            this.update(cx, |s, cx| {
                if s.online.epoch == epoch {
                    s.online.seasons = Load::from(seasons);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let tracks = cx.background_executor().spawn(async move { b.tracks() }).await;
            this.update(cx, |s, cx| {
                if s.online.epoch == epoch {
                    s.online.tracks = Load::from(tracks);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        if let Some(key) = self.online.open.clone() {
            self.online_open_board(key, cx);
        }
        cx.notify();
    }

    pub(crate) fn online_open_board(&mut self, key: BoardKey, cx: &mut Context<Self>) {
        self.online.epoch += 1;
        let epoch = self.online.epoch;
        self.online.open = Some(key.clone());
        self.online.board = Load::Loading;
        self.online.driver = None;
        self.online.viewing = None;
        let api = self.online.api.clone();
        cx.spawn(async move |this, cx| {
            let k = key.clone();
            let board = cx
                .background_executor()
                .spawn(async move { api.board(&k.content, &k.track, &k.weather, PAGE_SIZE, 0) })
                .await;
            this.update(cx, |s, cx| {
                if s.online.epoch == epoch && s.online.open.as_ref() == Some(&key) {
                    s.online.board = Load::from(board);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn online_more(&mut self, cx: &mut Context<Self>) {
        let (Some(key), Load::Ready(board)) = (self.online.open.clone(), &self.online.board) else {
            return;
        };
        if self.online.more {
            return;
        }
        self.online.more = true;
        let offset = board.rows.len() as u32;
        let epoch = self.online.epoch;
        let api = self.online.api.clone();
        cx.spawn(async move |this, cx| {
            let k = key.clone();
            let next = cx
                .background_executor()
                .spawn(async move { api.board(&k.content, &k.track, &k.weather, PAGE_SIZE, offset) })
                .await;
            this.update(cx, |s, cx| {
                s.online.more = false;
                if s.online.epoch == epoch
                    && let (Ok(next), Load::Ready(board)) = (next, &mut s.online.board)
                {
                    board.total = next.total;
                    board.rows.extend(next.rows);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn online_close_board(&mut self, cx: &mut Context<Self>) {
        self.online.epoch += 1;
        self.online.open = None;
        self.online.board = Load::Idle;
        self.online.driver = None;
        self.online.viewing = None;
        cx.notify();
    }

    pub(crate) fn online_open_driver(&mut self, user: User, cx: &mut Context<Self>) {
        self.online.epoch += 1;
        let epoch = self.online.epoch;
        let id = user.id.clone();
        self.online.driver = Some((user, Load::Loading));
        self.online.viewing = None;
        let api = self.online.api.clone();
        cx.spawn(async move |this, cx| {
            let who = id.clone();
            let driver = cx
                .background_executor()
                .spawn(async move { api.driver(&who) })
                .await;
            this.update(cx, |s, cx| {
                if s.online.epoch == epoch
                    && let Some((u, state)) = &mut s.online.driver
                    && u.id == id
                {
                    *state = Load::from(driver);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn online_close_driver(&mut self, cx: &mut Context<Self>) {
        self.online.driver = None;
        self.online.viewing = None;
        cx.notify();
    }

    pub(crate) fn online_game_running(&self) -> bool {
        self.online.pretend_running || matches!(self.play_state(), crate::shell::PlayState::Running)
    }
}
