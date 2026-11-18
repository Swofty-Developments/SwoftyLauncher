use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::install::Game;
use swofty_core::online::multiplayer::client::{self as diag, Local, Session};
use swofty_core::online::multiplayer::types::{GameStatus, RoomSummary, SessionState};
use swofty_core::store::Store;
use swofty_i18n::tr;

use super::model::{exit_leaves, exit_notice};
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted, reports_nothing};

pub(super) const TICK: Duration = Duration::from_millis(500);
pub(super) const GAME_EVERY: Duration = Duration::from_secs(2);
pub(super) const ROOMS_EVERY: Duration = Duration::from_secs(5);
pub(super) const MIN_PLAYERS: u8 = 2;
pub(super) const MAX_PLAYERS: u8 = 24;
pub(super) const AVATARS_DIR: &str = "avatars";

pub(super) fn read_local(store: &Store, profile: &str, dir: &Path) -> Result<Local, String> {
    let game = Game::locate(&store.root, dir.to_path_buf()).map_err(|e| e.to_string())?;
    let profile = store.profile(profile).map_err(|e| e.to_string())?;
    Local::read(store, &profile, &game.identity).map_err(|e| e.to_string())
}

pub(super) fn read_game(dir: &Path, running: bool) -> Option<GameStatus> {
    diag::game_status(dir, running)
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
    pub(super) closing: Option<Vec<u32>>,
    pub(super) game_seen: bool,
    pub(super) error: Option<String>,
    pub(super) copied: Option<Instant>,
    pub(super) launch_seen: u32,
    pub(super) room_launching: bool,
    pub(super) held: Option<RoomLaunch>,
    pub(super) faces: HashMap<String, Arc<Image>>,
    pub(super) faces_asked: HashSet<String>,
    pub(super) _inputs: [Subscription; 2],
    pub(super) lobby: Lobby,
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
        Self {
            visible: false,
            refused: None,
            form: false,
            name,
            code,
            public: true,
            max: 8,
            starting: None,
            session: None,
            state: None,
            leaving: false,
            viewing: None,
            notice: None,
            closing: None,
            game_seen: false,
            error: None,
            copied: None,
            launch_seen: 0,
            room_launching: false,
            held: None,
            faces: HashMap::new(),
            faces_asked: HashSet::new(),
            _inputs: inputs,
            lobby: Lobby::default(),
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.starting.is_some() || self.session.is_some()
    }

    pub(super) fn finish_leaving(&mut self) {
        self.session = None;
        self.launch_seen = 0;
        self.room_launching = false;
        self.held = None;
        self.state = None;
        self.leaving = false;
        self.closing = None;
        self.game_seen = false;
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
    pub(super) digest: Option<String>,
    pub(super) digest_busy: bool,
}

impl Lobby {
    /// Polls everything again at once, as when the page is shown or a room is left.
    pub(super) fn refresh(&mut self) {
        self.rooms_at = None;
        self.game_at = None;
        self.digest = None;
    }

    pub(super) fn game_due(&self, now: Instant) -> bool {
        !self.game_polling && due(self.game_at, GAME_EVERY, now)
    }

    pub(super) fn rooms_due(&self, now: Instant) -> bool {
        !self.rooms_polling && due(self.rooms_at, ROOMS_EVERY, now)
    }

    pub(super) fn digest_due(&self) -> bool {
        self.digest.is_none() && !self.digest_busy
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

    pub(super) fn took_digest(&mut self, digest: Result<String, String>) {
        self.digest_busy = false;
        match digest {
            Ok(d) => self.digest = Some(d),
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
    /// A prompt Play opened holds the room launch: its Play goes on with it through `Shell::play`.
    pub(crate) fn hold(&mut self, room: RoomLaunch) {
        self.held = Some(room);
    }

    pub(crate) fn take_held(&mut self) -> Option<RoomLaunch> {
        self.held.take()
    }

    /// Drops a held room launch once its prompt closed without Play. True when one was dropped.
    pub(crate) fn drop_held(&mut self) -> bool {
        if self.held.take().is_none() {
            return false;
        }
        self.room_launching = false;
        if let Some(session) = &self.session {
            session.launch_failed("Play was cancelled");
        }
        true
    }

    pub(crate) fn launch_result(&mut self, started: bool) {
        let Some(session) = &self.session else {
            return;
        };
        if started {
            session.launched();
        } else {
            self.room_launching = false;
            session.launch_failed("see the notice, finish what it asks, then try again");
        }
    }

    /// The launch `launch` started ended without the game: `why` says what stopped it.
    pub(crate) fn launch_ended(&mut self, why: Option<String>) {
        if !std::mem::take(&mut self.room_launching) {
            return;
        }
        match why {
            None => self.game_seen = true,
            Some(why) => {
                if let Some(session) = &self.session {
                    session.launch_failed(&why);
                }
            }
        }
    }

    /// Leaves the room. True when there was one to leave.
    pub(crate) fn leave(&mut self) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        session.leave();
        self.leaving = true;
        true
    }

    /// Follows the game for the room: once the game the room started has exited, the room is left.
    /// True when it left.
    pub(crate) fn watch_game(&mut self, running: bool, busy: bool) -> bool {
        if self.session.is_none() {
            return false;
        }
        if running && self.launch_seen != 0 && self.closing.is_none() {
            self.game_seen = true;
        }
        if self.closing.is_some() && !running {
            self.closing = None;
        }
        let over = self.state.as_ref().is_some_and(|s| s.phase.is_over());
        if over || self.leaving || !exit_leaves(self.game_seen, running, busy) {
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
}

pub(crate) struct OnlineNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) profile: &'a str,
    pub(crate) game_dir: Option<&'a Path>,
    pub(crate) running: bool,
    /// A launch, a build or a restore of the game folder is running.
    pub(crate) busy: bool,
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
                busy: self.launching.launch.is_some()
                    || self.game.build.is_some()
                    || self.launching.restoring,
                token: self.account.session.as_ref().map(|m| m.token.as_str()),
            },
        )
    }

    reports_nothing!();
}
