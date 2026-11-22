use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use gpui_kit::component::WindowExt;
use gpui_kit::*;
use swofty_core::catalog;
use swofty_core::online::multiplayer::client::{self as diag, Session, Setup};
use swofty_core::online::multiplayer::matching::{self, ROOM_PROFILE};
use swofty_core::online::multiplayer::types::{RoomSettings, RoomSummary};
use swofty_i18n::{tr, trf};

use super::fix;
use super::model::*;
use super::room_game::Withdrawn;
use super::state::*;
use super::steward;
use super::tracker::{PlayWord, play_word};
use crate::shell::Launching;
use crate::shell::host::Host;
use crate::shell::host::PromptHost;
use crate::shell::{Page, Shell};
use crate::ui::monogram;
use crate::ui::theme::Palette;

impl Shell {
    pub(crate) fn watch_multiplayer(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update_in(cx, Multiplayer::tick).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn room_begin(&mut self, start: Start, window: &mut Window, cx: &mut Context<Self>) {
        if self.rooms.busy() || !Multiplayer::close_game(self, window, cx) {
            return;
        }
        self.rooms.notice = None;
        Multiplayer::start(self, start, cx);
    }

    pub(super) fn room_play(&mut self, start: Start, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.game.install.as_ref().map(|i| i.dir.clone());
        let running = dir
            .as_ref()
            .is_some_and(|d| self.game.processes.iter().any(|g| g.is_in(d)));
        if !running {
            self.room_begin(start, window, cx);
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
                        .update(cx, |s, cx| s.room_begin(start.clone(), window, cx))
                        .ok();
                    true
                })
        });
    }

    pub(crate) fn room_view_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(code) = self.rooms.viewing.clone() else {
            return;
        };
        self.go(Page::Multiplayer, cx);
        self.room_play(Start::Join(code), window, cx);
    }

    pub(crate) fn room_viewed(&self) -> Option<&RoomSummary> {
        let code = self.rooms.viewing.as_deref()?;
        self.rooms.lobby.rooms.iter().find(|r| r.code == code)
    }

    /// The play button while a shared session runs and F1 25 isn't building or running yet, so it can't be
    /// pressed as a second Play.
    pub(crate) fn room_play_word(&self) -> Option<PlayWord> {
        let m = &self.rooms;
        play_word(m.busy(), m.leaving, m.state.as_ref())
    }

    pub(crate) fn room_starting_label(&self) -> Option<&'static str> {
        self.room_play_word().map(PlayWord::label)
    }

    pub(crate) fn room_can_play(&self) -> bool {
        self.navigation.page == Page::Multiplayer
            && self.account.allowed()
            && !self.rooms.busy()
            && self.room_viewed().is_some_and(|room| {
                RoomStatus::of(room, &self.room_enabled_addons(), self.rooms.lobby.print.as_ref()).joinable()
            })
    }

    pub(crate) fn face(&self, name: &str, url: Option<&str>, size: f32, p: Palette) -> Div {
        match url.and_then(|u| self.rooms.faces.get(u)) {
            Some(image) => div()
                .size(px(size))
                .flex_shrink_0()
                .rounded_full()
                .overflow_hidden()
                .child(img(image.clone()).size_full()),
            None => monogram(name, size, p).rounded_full(),
        }
    }

    /// A picture fetched with [`Multiplayer::load_faces`], once it is in.
    pub(crate) fn fetched_image(&self, url: &str) -> Option<Arc<Image>> {
        self.rooms.faces.get(url).cloned()
    }

    pub(super) fn room_game_state(&self) -> GameState {
        let status = match &self.rooms.state {
            Some(state) if self.rooms.session.is_some() => state.game.as_ref(),
            _ => self.rooms.lobby.game.as_ref(),
        };
        GameState::of(!self.game.processes.is_empty(), status)
    }

    pub(super) fn room_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.rooms.name.read(cx).value().trim().to_owned();
        let name = if typed.is_empty() {
            self.room_default_name()
        } else {
            typed
        };
        let settings = RoomSettings {
            name: Some(name),
            public: self.rooms.public,
            max_members: self.rooms.max,
            ..RoomSettings::default()
        };
        self.room_play(Start::Create(settings), window, cx);
    }

    pub(super) fn room_default_name(&self) -> String {
        default_room_name(self.account.session.as_ref().map(|m| m.name.as_str()))
    }

    pub(super) fn room_open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rooms.form = !self.rooms.form;
        if self.rooms.form && self.rooms.name.read(cx).value().is_empty() {
            let name = self.room_default_name();
            self.rooms.name.update(cx, |s, cx| s.set_value(name, window, cx));
        }
        cx.notify();
    }

    pub(super) fn room_join_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = self.rooms.code.read(cx).value().trim().to_owned();
        if !code.is_empty() {
            self.room_play(Start::Join(code), window, cx);
        }
    }

    pub(crate) fn room_leave(&mut self, cx: &mut Context<Self>) {
        if self.rooms.leave() {
            cx.notify();
        }
    }

    pub(super) fn room_retry(&mut self, cx: &mut Context<Self>) {
        if self.rooms.fix.retry() {
            cx.notify();
            return;
        }
        if let Some(session) = &self.rooms.session {
            session.retry();
        }
        cx.notify();
    }

    pub(super) fn room_open(&mut self, code: String, cx: &mut Context<Self>) {
        self.rooms.viewing = Some(code);
        cx.notify();
    }

    pub(super) fn room_close_view(&mut self, cx: &mut Context<Self>) {
        self.rooms.viewing = None;
        cx.notify();
    }

    /// Back to the list of rooms from the room page: a room looked at closes, and a room joined keeps running.
    pub(crate) fn room_step_out(&mut self, cx: &mut Context<Self>) {
        match self.rooms.focus() {
            RoomFocus::InRoom => self.rooms.away = true,
            RoomFocus::Viewing(_) => self.rooms.viewing = None,
            RoomFocus::List => return,
        }
        cx.notify();
    }

    pub(crate) fn room_step_in(&mut self, cx: &mut Context<Self>) {
        self.rooms.away = false;
        self.open_page(Page::Multiplayer, cx);
    }

    pub(super) fn room_copy_code(&mut self, code: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        self.rooms.copied = Some(Instant::now());
        cx.notify();
    }
}

impl Multiplayer {
    pub(super) fn load_shown_faces<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let m = host.split().0;
        let members = m
            .state
            .iter()
            .flat_map(|s| &s.members)
            .filter_map(|x| x.avatar.as_ref());
        let hosts = m.lobby.rooms.iter().filter_map(|r| r.host.avatar.as_ref());
        let wanted: Vec<String> = members.chain(hosts).cloned().collect();
        Self::load_faces(host, wanted, cx);
    }

    pub(crate) fn load_faces<H: Host<Self>>(host: &mut H, urls: Vec<String>, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        let wanted = unasked(urls, &m.faces_asked);
        if wanted.is_empty() {
            return;
        }
        m.faces_asked.extend(wanted.iter().cloned());
        let dir = needs.store.root.join(catalog::CACHE_DIR).join(AVATARS_DIR);
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
            this.update(cx, |host, cx| {
                host.split().0.faces.extend(faces);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn poll_game<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        let Some(dir) = needs.game_dir.map(Path::to_path_buf) else {
            m.lobby.no_game(Instant::now());
            return;
        };
        let running = needs.running;
        m.lobby.game_polling = true;
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move { read_game(&dir, running) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.lobby.took_game(status, Instant::now()) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn poll_rooms<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        let Some(token) = needs.token.map(str::to_owned) else {
            return;
        };
        m.lobby.rooms_polling = true;
        let data = needs.store.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    diag::load(&data, Some(&token)).map_err(|e| e.to_string())?;
                    diag::rooms(&token)
                })
                .await;
            this.update(cx, |host, cx| {
                host.split().0.lobby.took_rooms(result, Instant::now());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn read_print<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        let Some(dir) = needs.game_dir.map(Path::to_path_buf) else {
            return;
        };
        let (store, profile) = (needs.store.clone(), needs.profile.to_owned());
        m.lobby.print_busy = true;
        cx.spawn(async move |this, cx| {
            let print = cx
                .background_executor()
                .spawn(async move { read_own(&store, &profile, &dir) })
                .await;
            this.update(cx, |host, cx| {
                host.split().0.lobby.took_print(print);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens or joins a room off the UI thread with this profile's addons.
    pub(super) fn start<H: Host<Self>>(host: &mut H, start: Start, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        if m.busy() {
            return;
        }
        let Some(token) = needs.token.map(str::to_owned) else {
            return;
        };
        let Some(dir) = needs.game_dir.map(Path::to_path_buf) else {
            m.error = Some("F1 25 was not found. Locate it from the play bar first.".into());
            cx.notify();
            return;
        };
        let (store, profile) = (needs.store.clone(), needs.profile.to_owned());
        let listed = match &start {
            Start::Join(code) => m
                .lobby
                .rooms
                .iter()
                .find(|r| r.code == *code)
                .and_then(|r| r.addons.as_ref())
                .and_then(matching::listed_addons),
            Start::Create(_) => None,
        };
        m.starting = Some(starting(&start));
        m.error = None;
        let flag = Arc::new(AtomicBool::new(false));
        m.roomed = flag.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    diag::load(&store.root, Some(&token)).map_err(|e| e.to_string())?;
                    let joining = matches!(start, Start::Join(_));
                    let (read, roomed) = match (&listed, joining) {
                        (Some(addons), true) => (room_local(&store, &profile, &dir, addons), true),
                        (_, false) => (read_local(&store, &profile, &dir), false),
                        (None, true) => match read_local(&store, &profile, &dir) {
                            Ok(local) => (Ok(local), false),
                            Err(_) => (room_local(&store, &profile, &dir, &[]), true),
                        },
                    };
                    let local = read
                        .inspect_err(|e| {
                            diag::line(format!("session not started: couldn't read the addons: {e}"));
                        })
                        .map_err(|e| trf!("Couldn't read your addons: {e}", e = e).to_string())?;
                    let mut setup = Setup::new(&token, &dir, local);
                    setup.room_launch = true;
                    flag.store(roomed, Ordering::Relaxed);
                    let (refresh_store, refresh_profile, refresh_dir) =
                        (store.clone(), profile.clone(), dir.clone());
                    setup.refresh = Some(Arc::new(move || {
                        let played = if flag.load(Ordering::Relaxed) {
                            ROOM_PROFILE
                        } else {
                            refresh_profile.as_str()
                        };
                        read_local(&refresh_store, played, &refresh_dir).ok()
                    }));
                    match start {
                        Start::Create(settings) => Session::create(setup, &settings),
                        Start::Join(code) => Session::join(setup, &code),
                    }
                })
                .await;
            this.update(cx, |host, cx| {
                let m = host.split().0;
                m.starting = None;
                match result {
                    Ok(session) => {
                        m.state = Some(session.state());
                        m.session = Some(session);
                        m.form = false;
                        m.away = false;
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

    /// The online page's tick: the room's launch and game watch, leaving, and the page's polls while it
    /// shows.
    pub(super) fn tick<H: Host<Self> + PromptHost<Launching>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) {
        Self::withdraw(host, window, cx);
        Self::launch(host, window, cx);
        Self::match_host(host, cx);
        let dialog = window.has_active_dialog(cx);
        let (m, needs) = <H as Host<Self>>::split(host);
        let mut changed = !dialog && m.drop_held();
        changed |= m.watch_game(
            needs.running,
            needs.busy,
            || session_ended(needs.store),
            Instant::now(),
        );
        if m.leaving && m.session.as_ref().is_none_or(Session::is_finished) {
            m.finish_leaving();
            changed = true;
        }
        if !needs.shown {
            m.visible = false;
            if changed {
                cx.notify();
            }
            return;
        }
        let now = Instant::now();
        if !m.visible {
            m.visible = true;
            m.lobby.refresh();
        }
        let refused = refusal(m.refused.clone(), diag::is_loaded(), diag::unavailable());
        if refused != m.refused {
            if refused.is_none() {
                m.lobby.rooms_at = None;
            }
            m.refused = refused;
            changed = true;
        }
        if let Some(session) = &m.session {
            let mut state = session.state();
            if let Some(fix) = m.fix.view() {
                fix::patch(&mut state, &fix);
            }
            if m.state.as_ref() != Some(&state) {
                m.state = Some(state);
                changed = true;
            }
        }
        if m.copied
            .is_some_and(|at| now.duration_since(at) > Duration::from_secs(2))
        {
            m.copied = None;
            changed = true;
        }
        let offered = m.steward_offered(needs.running);
        if !offered {
            changed |= m.steward.forget();
        }
        if changed {
            cx.notify();
        }
        if offered && m.steward.due(now) {
            Self::read_steward(host, cx);
        }
        let m = <H as Host<Self>>::split(host).0;
        if m.session.is_none() && m.lobby.game_due(now) {
            Self::poll_game(host, cx);
        }
        let m = <H as Host<Self>>::split(host).0;
        if m.session.is_none() && m.lobby.rooms_due(now) {
            Self::poll_rooms(host, cx);
        }
        let m = <H as Host<Self>>::split(host).0;
        if m.session.is_none() && m.lobby.print_due() {
            Self::read_print(host, cx);
        }
        Self::load_shown_faces(host, cx);
    }

    /// Matches the host's addons once the session has them: installs from the Addon Browser, then the room
    /// profile, off the UI thread. The room launch waits for it ([`super::fix::RoomFix::holds`]).
    pub(super) fn match_host<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        let Some(session) = &m.session else {
            return;
        };
        m.host_print = session.host_print();
        let launched = m.game.launching || needs.running;
        let joiner = m.state.as_ref().is_some_and(|s| !s.host);
        if !joiner || !m.fix.due(m.host_print.as_ref(), launched) {
            return;
        }
        let (Some(print), Some(dir)) = (m.host_print.clone(), needs.game_dir.map(Path::to_path_buf)) else {
            return;
        };
        let progress = m.fix.begin(&print.digest);
        diag::line(format!(
            "launcher: matching the host's {} addon(s) for the room",
            print.addons.len()
        ));
        let (store, profile) = (needs.store.clone(), needs.profile.to_owned());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { fix::run(&store, &profile, &dir, &print, &progress) })
                .await;
            this.update(cx, |host, cx| {
                let m = host.split().0;
                let result = match result {
                    Ok((local, replaces)) => {
                        diag::line("launcher: the room profile has the host's addons");
                        for line in &replaces {
                            diag::line(format!("launcher: {line}"));
                        }
                        m.roomed.store(true, Ordering::Relaxed);
                        if let Some(session) = &m.session {
                            session.update_local(&local);
                        }
                        Ok(())
                    }
                    Err(why) => {
                        diag::line(format!("launcher: couldn't match the host's addons: {why}"));
                        Err(why)
                    }
                };
                m.fix.finish(result, Instant::now());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Reads the League Stewarding page the host's game names, off the UI thread.
    pub(super) fn read_steward<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (m, needs) = host.split();
        let Some(dir) = needs.game_dir.map(Path::to_path_buf) else {
            return;
        };
        let started = steward::process_started(needs.processes, &dir);
        let store = needs.store.clone();
        m.steward.reading = true;
        cx.spawn(async move |this, cx| {
            let url = cx
                .background_executor()
                .spawn(async move { steward::read(&store, &dir, started) })
                .await;
            this.update(cx, |host, cx| {
                let (m, needs) = host.split();
                let offered = m.steward_offered(needs.running);
                if m.steward.took(url, offered, Instant::now()) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Closes the copy's F1 25 so the room can start it. False when a stop failed, which a notice says.
    pub(super) fn close_game<H: Host<Self>>(host: &mut H, window: &mut Window, cx: &mut Context<H>) -> bool {
        let (m, needs) = host.split();
        let Some(dir) = needs.game_dir else {
            return true;
        };
        let pids: Vec<u32> = needs
            .processes
            .iter()
            .filter(|g| g.is_in(dir))
            .map(|g| g.pid)
            .collect();
        if pids.is_empty() {
            return true;
        }
        swofty_core::session::request_stop(&needs.store.root);
        for pid in &pids {
            if let Err(e) = swofty_core::platform::process::stop_process(*pid) {
                crate::app::app_log::action("stop f1 25").failed(&e);
                window.notify(Notification::error(e.to_string()), cx);
                return false;
            }
        }
        m.game.closing = Some(pids);
        true
    }

    /// Cancels the room's Play while it waits on its checks or a prompt once the session no longer asks for
    /// it: the room ended, is being left, or the session withdrew its ask.
    fn withdraw<H: Host<Self> + PromptHost<Launching>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) {
        let m = <H as Host<Self>>::split(host).0;
        let Some(withdrawn) = m.game.withdraw(m.session.as_ref(), m.leaving) else {
            return;
        };
        diag::line("launch: the session no longer asks for F1 25; the room's Play is cancelled");
        match withdrawn {
            Withdrawn::Checks => <H as Host<Launching>>::split(host).0.play_pending.cancel(),
            Withdrawn::Prompt if window.has_active_dialog(cx) => window.close_dialog(cx),
            Withdrawn::Prompt => {}
        }
        cx.notify();
    }

    /// Starts F1 25 for the room when the session asks, the game is closed and the launcher has put the
    /// game folder back.
    pub(super) fn launch<H: Host<Self> + PromptHost<Launching>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) {
        let (m, needs) = <H as Host<Self>>::split(host);
        let Some(session) = &m.session else {
            return;
        };
        if m.fix.holds(m.host_print.as_ref(), Instant::now()) {
            return;
        }
        let Some(ask) = m.game.due(session, needs.busy || needs.running) else {
            return;
        };
        if let Some(dir) = needs.game_dir {
            diag::forget_status(dir);
        }
        let Some(mut room) = m.game.begin(session, ask, &needs.store.root) else {
            return;
        };
        if m.roomed.load(Ordering::Relaxed) {
            room.profile = Some(ROOM_PROFILE.to_owned());
        }
        let started = Launching::play_for(host, Some(room), window, cx);
        if !started {
            <H as Host<Self>>::split(host).0.launch_result(false);
        }
        cx.notify();
    }
}
