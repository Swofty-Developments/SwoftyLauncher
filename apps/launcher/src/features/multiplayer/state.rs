use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::diagnostics::send_logs::RoomRun;
use swofty_core::install::Game;
use swofty_core::online::multiplayer::client::{self as diag, Local, Session};
use swofty_core::online::multiplayer::types::{GameStatus, RoomSettings, RoomSummary, SessionState};
use swofty_core::platform::process::GameProcess;
use swofty_core::store::Store;
use swofty_i18n::tr;

use super::model::{OwnPrint, exit_notice, leaves_on_exit};
use super::preview::{self, RoomPreview};
use super::room_game::RoomGame;
use super::steward::{self, Steward};
use crate::shell::host::{Host, Hosted, reports_nothing};
use crate::shell::{Page, Shell};

pub(super) const TICK: Duration = Duration::from_millis(500);
pub(super) const GAME_EVERY: Duration = Duration::from_secs(2);
pub(super) const ROOMS_EVERY: Duration = Duration::from_secs(5);
/// How long the room waits after its game exited for the game's session to write how it ended.
pub(super) const EXIT_WAIT: Duration = Duration::from_secs(5);
pub(super) const MIN_PLAYERS: u8 = 2;
pub(super) const MAX_PLAYERS: u8 = 24;
pub(super) const AVATARS_DIR: &str = "avatars";

/// Every room size the create form offers, as the room server accepts them.
pub(super) fn room_sizes() -> RangeInclusive<u8> {
    MIN_PLAYERS..=MAX_PLAYERS
}

pub(super) fn read_local(store: &Store, profile: &str, dir: &Path) -> Result<Local, String> {
    let game = Game::locate(&store.root, dir.to_path_buf()).map_err(|e| e.to_string())?;
    let profile = store.profile(profile).map_err(|e| e.to_string())?;
    Local::read(store, &profile, &game.identity).map_err(|e| e.to_string())
}

pub(super) fn read_game(dir: &Path, running: bool) -> Option<GameStatus> {
    diag::game_status(dir, running)
}

/// Whether the newest game session wrote how its game ended.
pub(super) fn session_ended(store: &Store) -> bool {
    store
        .latest_session()
        .is_some_and(|dir| dir.join(swofty_core::session::END_FILE).is_file())
}

pub(crate) struct RoomLaunch {
    pub(crate) row_ids: Option<PathBuf>,
    /// The direct start request the game reads at boot, as `swofty\direct_start.json`: the room target, this
    /// player's role, the host's address and the room configuration (`LaunchAsk::direct_start`).
    pub(crate) direct_start: String,
}

pub(crate) struct Multiplayer {
    pub(super) visible: bool,
    pub(super) refused: Option<String>,
    pub(super) form: bool,
    pub(super) name: Entity<InputState>,
    pub(super) code: Entity<InputState>,
    pub(super) public: bool,
    pub(super) max: u8,
    pub(super) starting: Option<String>,
    pub(super) session: Option<Session>,
    pub(super) state: Option<SessionState>,
    pub(super) leaving: bool,
    pub(super) viewing: Option<String>,
    pub(super) notice: Option<String>,
    pub(super) error: Option<String>,
    pub(super) copied: Option<Instant>,
    pub(crate) game: RoomGame,
    pub(super) faces: HashMap<String, Arc<Image>>,
    pub(super) faces_asked: HashSet<String>,
    pub(super) _inputs: [Subscription; 2],
    pub(super) lobby: Lobby,
    pub(super) steward: Steward,
    pub(super) preview: Option<RoomPreview>,
    pub(super) ad_stand_in: bool,
}

impl Multiplayer {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Room name")));
        let code = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Invite code")));
        let changed = |cx: &mut Context<Shell>, input: &Entity<InputState>| {
            cx.subscribe(input, |_, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    cx.notify();
                }
            })
        };
        let inputs = [changed(cx, &name), changed(cx, &code)];
        let debug = cfg!(debug_assertions);
        let (state, preview) = preview::room(debug, std::env::var_os(preview::ROOM_ENV).as_deref()).unzip();
        let mut lobby = Lobby::default();
        if let Some(rooms) = preview::rooms(debug, std::env::var_os(preview::ROOMS_ENV).as_deref()) {
            lobby.rooms = rooms;
            lobby.rooms_loaded = true;
        }
        Self {
            visible: false,
            refused: None,
            form: false,
            name,
            code,
            public: true,
            max: RoomSettings::default().max_members,
            starting: None,
            session: None,
            state,
            leaving: false,
            viewing: None,
            notice: None,
            error: None,
            copied: None,
            game: RoomGame::default(),
            faces: HashMap::new(),
            faces_asked: HashSet::new(),
            _inputs: inputs,
            lobby,
            steward: Steward::default(),
            preview,
            ad_stand_in: preview::ad_stand_in(debug, std::env::var_os(preview::AD_ENV).as_deref()),
        }
    }

    /// Whether the page shows a room: one this launcher is in, or a previewed one.
    pub(super) fn in_room(&self) -> bool {
        self.session.is_some() || self.preview.is_some()
    }

    /// League Stewarding's page, when the room page offers it.
    pub(super) fn steward_url(&self) -> Option<String> {
        let previewed = self.preview.as_ref().and_then(|p| p.steward.clone());
        self.steward.url.clone().or(previewed)
    }

    /// Whether the room page offers League Stewarding's page, with the game `running`.
    pub(super) fn steward_offered(&self, running: bool) -> bool {
        let state = self.state.as_ref();
        steward::offered(
            self.session.is_some() && !self.leaving,
            state.is_some_and(|s| s.host),
            state.is_some_and(|s| s.phase.is_over()),
            running,
        )
    }

    pub(super) fn busy(&self) -> bool {
        self.starting.is_some() || self.session.is_some()
    }

    pub(super) fn finish_leaving(&mut self) {
        self.session = None;
        self.game = RoomGame::default();
        self.state = None;
        self.leaving = false;
        self.viewing = None;
        self.lobby.refresh();
    }
}

pub(super) fn due(at: Option<Instant>, every: Duration, now: Instant) -> bool {
    at.is_none_or(|at| now.duration_since(at) >= every)
}

/// What the page polls while no room is joined: the game, the room list and this profile's addons.
#[derive(Default)]
pub(crate) struct Lobby {
    pub(super) game: Option<GameStatus>,
    pub(super) game_at: Option<Instant>,
    pub(super) game_polling: bool,
    pub(super) rooms: Vec<RoomSummary>,
    pub(super) rooms_loaded: bool,
    pub(super) rooms_error: Option<String>,
    pub(super) rooms_at: Option<Instant>,
    pub(super) rooms_polling: bool,
    pub(super) print: Option<OwnPrint>,
    pub(super) print_busy: bool,
}

impl Lobby {
    /// Polls everything again at once, as when the page is shown or a room is left.
    pub(super) fn refresh(&mut self) {
        self.rooms_at = None;
        self.game_at = None;
        self.print = None;
    }

    pub(super) fn game_due(&self, now: Instant) -> bool {
        !self.game_polling && due(self.game_at, GAME_EVERY, now)
    }

    pub(super) fn rooms_due(&self, now: Instant) -> bool {
        !self.rooms_polling && due(self.rooms_at, ROOMS_EVERY, now)
    }

    pub(super) fn print_due(&self) -> bool {
        self.print.is_none() && !self.print_busy
    }

    pub(super) fn no_game(&mut self, now: Instant) {
        self.game = None;
        self.game_at = Some(now);
    }

    /// Takes a read of the game's status; true when it changed.
    pub(super) fn took_game(&mut self, status: Option<GameStatus>, now: Instant) -> bool {
        self.game_polling = false;
        self.game_at = Some(now);
        if self.game == status {
            return false;
        }
        self.game = status;
        true
    }

    pub(super) fn took_rooms(&mut self, result: Result<Vec<RoomSummary>, String>, now: Instant) {
        self.rooms_polling = false;
        self.rooms_at = Some(now);
        match result {
            Ok(rooms) => {
                self.rooms = rooms;
                self.rooms_loaded = true;
                self.rooms_error = None;
            }
            Err(e) => self.rooms_error = Some(e),
        }
    }

    pub(super) fn took_print(&mut self, print: Result<OwnPrint, String>) {
        self.print_busy = false;
        match print {
            Ok(p) => self.print = Some(p),
            Err(e) => eprintln!("swofty-launcher: couldn't read this profile's addons: {e}"),
        }
    }
}

/// The web avatars in `urls` not asked for yet, each once.
pub(super) fn unasked(urls: Vec<String>, asked: &HashSet<String>) -> Vec<String> {
    urls.into_iter()
        .filter(|u| u.starts_with("http") && !asked.contains(u))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

impl Multiplayer {
    /// Drops a held room launch once its prompt closed without Play. True when one was dropped.
    pub(crate) fn drop_held(&mut self) -> bool {
        self.game.drop_held(self.session.as_ref())
    }

    pub(crate) fn launch_result(&mut self, started: bool) {
        self.game.launch_result(self.session.as_ref(), started);
    }

    /// The launch the room started ended without the game: `why` says what stopped it.
    pub(crate) fn launch_ended(&mut self, why: Option<String>) {
        self.game.launch_ended(self.session.as_ref(), why);
    }

    /// Leaves the room. True when there was one to leave.
    pub(crate) fn leave(&mut self) -> bool {
        if self.preview.take().is_some() {
            self.state = None;
            return true;
        }
        let Some(session) = &self.session else {
            return false;
        };
        session.leave();
        self.leaving = true;
        true
    }

    /// Follows the game for the room: once the game the room started has exited and its session wrote how it
    /// ended, or [`EXIT_WAIT`] passed, the room is left, unless the room's session takes the exit as a failed
    /// step ([`leaves_on_exit`]). True when it left.
    pub(crate) fn watch_game(
        &mut self,
        running: bool,
        busy: bool,
        end_written: impl FnOnce() -> bool,
        now: Instant,
    ) -> bool {
        if self.session.is_none() {
            return false;
        }
        let exited = self.game.exited(running, busy);
        let over = self.state.as_ref().is_some_and(|s| s.phase.is_over());
        if over || self.leaving || !exited || !leaves_on_exit(self.state.as_ref()) {
            return false;
        }
        if !self.game.exit_settled(end_written, now) {
            return false;
        }
        let host = self.state.as_ref().is_some_and(|s| s.host);
        self.notice = Some(exit_notice(host).to_owned());
        diag::line(format!(
            "launcher: F1 25 exited; leaving the room ({})",
            exit_notice(host)
        ));
        self.leave()
    }

    /// Tells the room's session how the game it ran ended, as the launcher judged it.
    pub(crate) fn game_exited(&self, exit_code: Option<i32>, stopped_by_launcher: bool, crashed: bool) {
        if let Some(session) = &self.session {
            session.game_exited(exit_code, stopped_by_launcher, crashed);
        }
    }

    /// The room the launcher is in, as a crashed game's logs name it: None outside a room, while leaving one,
    /// or once it is over.
    pub(crate) fn crash_room(&self) -> Option<RoomRun> {
        let state = self.state.as_ref();
        if self.session.is_none() || self.leaving || state.is_some_and(|s| s.phase.is_over()) {
            return None;
        }
        let word = |v: Option<serde_json::Value>| v.and_then(|v| v.as_str().map(str::to_owned));
        Some(RoomRun {
            code: state.and_then(|s| s.room.as_ref()).map(|r| r.code.clone()),
            step: word(
                state
                    .and_then(|s| s.progress.as_ref())
                    .and_then(|p| serde_json::to_value(p.step).ok()),
            ),
            phase: word(
                state
                    .and_then(|s| serde_json::to_value(&s.phase).ok())
                    .and_then(|v| v.get("phase").cloned()),
            ),
            host: state.is_some_and(|s| s.host),
            launched: self.game.seen,
        })
    }
}

pub(crate) struct OnlineNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) profile: &'a str,
    pub(crate) game_dir: Option<&'a Path>,
    pub(crate) running: bool,
    pub(crate) processes: &'a [GameProcess],
    /// A launch, a build or a restore of the game folder is running.
    pub(crate) busy: bool,
    /// The online page is open and the account may use it.
    pub(crate) shown: bool,
    pub(crate) token: Option<&'a str>,
}

impl Hosted for Multiplayer {
    type Needs<'a> = OnlineNeeds<'a>;
    type Change = Infallible;
}

impl Host<Multiplayer> for Shell {
    fn split(&mut self) -> (&mut Multiplayer, OnlineNeeds<'_>) {
        (
            &mut self.rooms,
            OnlineNeeds {
                store: &self.store,
                profile: &self.library.profile.name,
                game_dir: self.game.install.as_ref().map(|i| i.dir.as_path()),
                running: !self.game.processes.is_empty(),
                processes: &self.game.processes,
                busy: self.launching.launch.is_some()
                    || self.game.build.is_some()
                    || self.launching.restoring,
                shown: self.navigation.page == Page::Multiplayer && self.account.allowed(),
                token: self.account.session.as_ref().map(|m| m.token.as_str()),
            },
        )
    }

    reports_nothing!();
}
