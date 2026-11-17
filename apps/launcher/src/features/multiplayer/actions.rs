use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use gpui_kit::component::WindowExt;
use gpui_kit::*;
use swofty_core::catalog;
use swofty_core::multiplayer::client::{self as diag, Session, Setup};
use swofty_core::multiplayer::types::{RoomSettings, RoomSummary};
use swofty_i18n::{tr, trf};

use super::model::*;
use super::state::*;
use crate::shell::pages::monogram;
use crate::shell::{Page, Shell};
use crate::ui::theme::Palette;

impl Shell {
    pub(crate) fn mp_allowed(&self) -> bool {
        self.mp.as_ref().is_some_and(|m| m.allowed)
    }

    pub(crate) fn watch_multiplayer(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update_in(cx, |s, window, cx| s.mp_tick(window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn mp_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mp_room_launch(window, cx);
        self.mp_watch_game(cx);
        let m = &mut self.mp_page;
        if m.leaving && m.session.as_ref().is_none_or(Session::is_finished) {
            m.finish_leaving();
            cx.notify();
        }
        if self.page != Page::Multiplayer || !self.mp_allowed() {
            self.mp_page.visible = false;
            return;
        }
        let now = Instant::now();
        let m = &mut self.mp_page;
        if !m.visible {
            m.visible = true;
            m.rooms_at = None;
            m.game_at = None;
            m.digest = None;
        }
        let refused = refusal(m.refused.clone(), diag::is_loaded(), diag::unavailable());
        if refused != m.refused {
            if refused.is_none() {
                m.rooms_at = None;
            }
            m.refused = refused;
            cx.notify();
        }
        if let Some(session) = &m.session {
            let state = session.state();
            if m.state.as_ref() != Some(&state) {
                m.state = Some(state);
                cx.notify();
            }
        }
        if m.copied
            .is_some_and(|at| now.duration_since(at) > Duration::from_secs(2))
        {
            m.copied = None;
            cx.notify();
        }
        if m.session.is_none() && !m.game_polling && due(m.game_at, GAME_EVERY, now) {
            self.mp_poll_game(cx);
        }
        let m = &self.mp_page;
        if m.session.is_none() && !m.rooms_polling && due(m.rooms_at, ROOMS_EVERY, now) {
            self.mp_poll_rooms(cx);
        }
        let m = &self.mp_page;
        if m.digest.is_none() && !m.digest_busy && m.session.is_none() {
            self.mp_read_digest(cx);
        }
        self.mp_load_faces(cx);
    }

    /// Starts F1 25 for the room when the session asks, the game is closed and the launcher has put the
    /// game folder back.
    pub(super) fn mp_room_launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = &self.mp_page.session else {
            return;
        };
        let Some(ask) = session.state().launch else {
            return;
        };
        if self.mp_page.launch_seen == ask.seq || self.launch.is_some() || self.game.build.is_some() {
            return;
        }
        if !self.game.processes.is_empty() || self.restoring {
            return;
        }
        self.mp_page.closing = None;
        if let Some(dir) = self.game.install.as_ref().map(|i| i.dir.clone()) {
            diag::forget_status(&dir);
        }
        let row_ids = match &ask.row_ids {
            Some(ids) => {
                let file = self.store.root.join("state/room-row-ids.json");
                if let Err(e) = std::fs::create_dir_all(self.store.root.join("state"))
                    .map_err(|e| e.to_string())
                    .and_then(|()| ids.write(&file).map_err(|e| e.to_string()))
                {
                    self.mp_page.launch_seen = ask.seq;
                    session.launch_failed(&format!("the host's ids and settings couldn't be written ({e})"));
                    return;
                }
                Some(file)
            }
            None => None,
        };
        self.mp_page.launch_seen = ask.seq;
        self.mp_page.game_seen = false;
        self.mp_page.room_launching = true;
        let direct_start = ask.direct_start.clone();
        let started = self.play_for(
            Some(RoomLaunch {
                row_ids,
                direct_start,
            }),
            window,
            cx,
        );
        if !started {
            self.mp_room_launch_result(false);
        }
        cx.notify();
    }

    pub(crate) fn mp_room_launch_result(&mut self, started: bool) {
        let Some(session) = &self.mp_page.session else {
            return;
        };
        if started {
            session.launched();
        } else {
            self.mp_page.room_launching = false;
            session.launch_failed("see the notice, finish what it asks, then try again");
        }
    }

    /// The launch `mp_room_launch` started ended without the game: `why` says what stopped it.
    pub(crate) fn mp_launch_ended(&mut self, why: Option<String>) {
        if !std::mem::take(&mut self.mp_page.room_launching) {
            return;
        }
        match why {
            None => self.mp_page.game_seen = true,
            Some(why) => {
                if let Some(session) = &self.mp_page.session {
                    session.launch_failed(&why);
                }
            }
        }
    }

    pub(super) fn mp_watch_game(&mut self, cx: &mut Context<Self>) {
        let busy = self.launch.is_some() || self.game.build.is_some() || self.restoring;
        let m = &mut self.mp_page;
        if m.session.is_none() {
            return;
        }
        if !self.game.processes.is_empty() && m.launch_seen != 0 && m.closing.is_none() {
            m.game_seen = true;
        }
        if m.closing.is_some() && self.game.processes.is_empty() {
            m.closing = None;
        }
        let over = m.state.as_ref().is_some_and(|s| s.phase.is_over());
        if over || m.leaving || !exit_leaves(m.game_seen, !self.game.processes.is_empty(), busy) {
            return;
        }
        let host = m.state.as_ref().is_some_and(|s| s.host);
        m.notice = Some(exit_notice(host).to_owned());
        diag::line(format!(
            "launcher: F1 25 exited; leaving the room ({})",
            exit_notice(host)
        ));
        self.mp_leave(cx);
    }

    pub(super) fn mp_close_game(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(dir) = self.game.install.as_ref().map(|i| i.dir.clone()) else {
            return true;
        };
        let pids: Vec<u32> = self
            .game
            .processes
            .iter()
            .filter(|g| g.is_in(&dir))
            .map(|g| g.pid)
            .collect();
        if pids.is_empty() {
            return true;
        }
        swofty_core::session::request_stop(&self.store.root);
        for pid in &pids {
            if let Err(e) = swofty_core::process::stop_process(*pid) {
                crate::app::app_log::action("stop f1 25").failed(&e);
                window.notify(Notification::error(e.to_string()), cx);
                return false;
            }
        }
        self.mp_page.closing = Some(pids);
        true
    }

    pub(super) fn mp_begin(&mut self, start: Start, window: &mut Window, cx: &mut Context<Self>) {
        if self.mp_page.busy() || !self.mp_close_game(window, cx) {
            return;
        }
        self.mp_page.notice = None;
        self.mp_start(start, cx);
    }

    pub(super) fn mp_play(&mut self, start: Start, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.game.install.as_ref().map(|i| i.dir.clone());
        let running = dir
            .as_ref()
            .is_some_and(|d| self.game.processes.iter().any(|g| g.is_in(d)));
        if !running {
            self.mp_begin(start, window, cx);
            return;
        }
        let shell = cx.entity().downgrade();
        let ok = restart_ok_text(&start);
        // The dialog's builder runs on every frame it draws, so each run gets its own copy of the start.
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, start) = (shell.clone(), start.clone());
            alert
                .title(tr("Restart F1 25?"))
                .description(tr(
                    "F1 25 is open, so it restarts for this room. Unsaved progress is lost.",
                ))
                .ok_text(ok)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    shell
                        .update(cx, |s, cx| s.mp_begin(start.clone(), window, cx))
                        .ok();
                    true
                })
        });
    }

    pub(crate) fn mp_view_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(code) = self.mp_page.viewing.clone() else {
            return;
        };
        self.go(Page::Multiplayer, cx);
        self.mp_play(Start::Join(code), window, cx);
    }

    pub(crate) fn mp_viewed_room(&self) -> Option<&RoomSummary> {
        let code = self.mp_page.viewing.as_deref()?;
        self.mp_page.rooms.iter().find(|r| r.code == code)
    }

    /// The play button's word while a shared session starts and F1 25 isn't building or running yet, so it
    /// can't be pressed as a second Play.
    pub(crate) fn mp_starting_label(&self) -> Option<&'static str> {
        let m = &self.mp_page;
        if !m.busy() {
            return None;
        }
        let hosting = m.starting.as_deref() == Some("create") || m.state.as_ref().is_some_and(|s| s.host);
        Some(if hosting {
            tr("Opening room…")
        } else {
            tr("Joining…")
        })
    }

    pub(crate) fn mp_can_play_room(&self) -> bool {
        self.page == Page::Multiplayer
            && self.mp_allowed()
            && !self.mp_page.busy()
            && self.mp_viewed_room().is_some()
    }

    pub(super) fn mp_load_faces(&mut self, cx: &mut Context<Self>) {
        let m = &mut self.mp_page;
        let members = m
            .state
            .iter()
            .flat_map(|s| &s.members)
            .filter_map(|x| x.avatar.as_ref());
        let hosts = m.rooms.iter().filter_map(|r| r.host.avatar.as_ref());
        let wanted: Vec<String> = members.chain(hosts).cloned().collect();
        self.load_faces(wanted, cx);
    }

    pub(crate) fn load_faces(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        let m = &mut self.mp_page;
        let wanted: Vec<String> = urls
            .into_iter()
            .filter(|u| u.starts_with("http") && !m.faces_asked.contains(u))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if wanted.is_empty() {
            return;
        }
        m.faces_asked.extend(wanted.iter().cloned());
        let dir = self.store.root.join(catalog::CACHE_DIR).join(AVATARS_DIR);
        cx.spawn(async move |this, cx| {
            let faces: Vec<(String, Arc<Image>)> = cx
                .background_executor()
                .spawn(async move {
                    wanted
                        .into_iter()
                        .filter_map(|url| crate::features::news::image(&dir, &url).map(|i| (url, i)))
                        .collect()
                })
                .await;
            this.update(cx, |s, cx| {
                s.mp_page.faces.extend(faces);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn face(&self, name: &str, url: Option<&str>, size: f32, p: Palette) -> Div {
        match url.and_then(|u| self.mp_page.faces.get(u)) {
            Some(image) => div()
                .size(px(size))
                .flex_shrink_0()
                .rounded_full()
                .overflow_hidden()
                .child(img(image.clone()).size_full()),
            None => monogram(name, size, p).rounded_full(),
        }
    }

    pub(super) fn mp_poll_game(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.game.install.as_ref().map(|i| i.dir.clone()) else {
            self.mp_page.game = None;
            self.mp_page.game_at = Some(Instant::now());
            return;
        };
        let running = !self.game.processes.is_empty();
        self.mp_page.game_polling = true;
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move { read_game(&dir, running) })
                .await;
            this.update(cx, |s, cx| {
                let m = &mut s.mp_page;
                m.game_polling = false;
                m.game_at = Some(Instant::now());
                if m.game != status {
                    m.game = status;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn mp_poll_rooms(&mut self, cx: &mut Context<Self>) {
        let Some(token) = self.mp.as_ref().map(|m| m.token.clone()) else {
            return;
        };
        self.mp_page.rooms_polling = true;
        let data = self.store.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    diag::load(&data, Some(&token)).map_err(|e| e.to_string())?;
                    diag::rooms(&token)
                })
                .await;
            this.update(cx, |s, cx| {
                let m = &mut s.mp_page;
                m.rooms_polling = false;
                m.rooms_at = Some(Instant::now());
                match result {
                    Ok(rooms) => {
                        m.rooms = rooms;
                        m.rooms_loaded = true;
                        m.rooms_error = None;
                    }
                    Err(e) => m.rooms_error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn mp_read_digest(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.game.install.as_ref().map(|i| i.dir.clone()) else {
            return;
        };
        let (store, profile) = (self.store.clone(), self.library.profile.name.clone());
        self.mp_page.digest_busy = true;
        cx.spawn(async move |this, cx| {
            let digest = cx
                .background_executor()
                .spawn(async move { read_local(&store, &profile, &dir).map(|l| l.digest()) })
                .await;
            this.update(cx, |s, cx| {
                s.mp_page.digest_busy = false;
                match digest {
                    Ok(d) => s.mp_page.digest = Some(d),
                    Err(e) => eprintln!("swofty-launcher: couldn't read this profile's addons: {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn mp_game_state(&self) -> GameState {
        let status = match &self.mp_page.state {
            Some(state) if self.mp_page.session.is_some() => state.game.as_ref(),
            _ => self.mp_page.game.as_ref(),
        };
        GameState::of(!self.game.processes.is_empty(), status)
    }

    pub(super) fn mp_start(&mut self, start: Start, cx: &mut Context<Self>) {
        if self.mp_page.busy() {
            return;
        }
        let Some(token) = self.mp.as_ref().map(|m| m.token.clone()) else {
            return;
        };
        let Some(dir) = self.game.install.as_ref().map(|i| i.dir.clone()) else {
            self.mp_page.error = Some("F1 25 was not found. Locate it from the play bar first.".into());
            cx.notify();
            return;
        };
        let (store, profile) = (self.store.clone(), self.library.profile.name.clone());
        let m = &mut self.mp_page;
        m.starting = Some(match &start {
            Start::Create(_) => "create".into(),
            Start::Join(code) => code.clone(),
        });
        m.error = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    diag::load(&store.root, Some(&token)).map_err(|e| e.to_string())?;
                    let local = read_local(&store, &profile, &dir)
                        .inspect_err(|e| {
                            diag::line(format!("session not started: couldn't read the addons: {e}"));
                        })
                        .map_err(|e| trf!("Couldn't read your addons: {e}", e = e).to_string())?;
                    let mut setup = Setup::new(&token, &dir, local);
                    setup.room_launch = true;
                    let (refresh_store, refresh_profile, refresh_dir) =
                        (store.clone(), profile.clone(), dir.clone());
                    setup.refresh = Some(Arc::new(move || {
                        read_local(&refresh_store, &refresh_profile, &refresh_dir).ok()
                    }));
                    match start {
                        Start::Create(settings) => Session::create(setup, &settings),
                        Start::Join(code) => Session::join(setup, &code),
                    }
                })
                .await;
            this.update(cx, |s, cx| {
                let m = &mut s.mp_page;
                m.starting = None;
                match result {
                    Ok(session) => {
                        m.state = Some(session.state());
                        m.session = Some(session);
                        m.form = false;
                    }
                    Err(e) => m.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn mp_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.mp_page.name.read(cx).value().trim().to_owned();
        let name = if typed.is_empty() {
            self.mp_default_name()
        } else {
            typed
        };
        let settings = RoomSettings {
            name: Some(name),
            public: self.mp_page.public,
            max_members: self.mp_page.max,
            ..RoomSettings::default()
        };
        self.mp_play(Start::Create(settings), window, cx);
    }

    pub(super) fn mp_default_name(&self) -> String {
        let you = self.mp.as_ref().map_or("Your", |m| m.name.as_str());
        format!("{you}'s room")
    }

    pub(super) fn mp_open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mp_page.form = !self.mp_page.form;
        if self.mp_page.form && self.mp_page.name.read(cx).value().is_empty() {
            let name = self.mp_default_name();
            self.mp_page
                .name
                .update(cx, |s, cx| s.set_value(name, window, cx));
        }
        cx.notify();
    }

    pub(super) fn mp_join_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = self.mp_page.code.read(cx).value().trim().to_owned();
        if !code.is_empty() {
            self.mp_play(Start::Join(code), window, cx);
        }
    }

    pub(crate) fn mp_leave(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &self.mp_page.session else {
            return;
        };
        session.leave();
        self.mp_page.leaving = true;
        cx.notify();
    }

    pub(super) fn mp_retry(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &self.mp_page.session {
            session.retry();
        }
        cx.notify();
    }

    pub(super) fn mp_open_room(&mut self, code: String, cx: &mut Context<Self>) {
        self.mp_page.viewing = Some(code);
        cx.notify();
    }

    pub(super) fn mp_close_view(&mut self, cx: &mut Context<Self>) {
        self.mp_page.viewing = None;
        cx.notify();
    }

    pub(super) fn mp_copy_code(&mut self, code: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        self.mp_page.copied = Some(Instant::now());
        cx.notify();
    }
}
