use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::install::Game;
use swofty_core::multiplayer::client::{self as diag, Local, Session};
use swofty_core::multiplayer::types::{GameStatus, RoomSummary, SessionState};
use swofty_core::store::Store;
use swofty_i18n::tr;

use crate::shell::Shell;

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
    pub(super) game: Option<GameStatus>,
    pub(super) game_at: Option<Instant>,
    pub(super) game_polling: bool,
    pub(super) rooms: Vec<RoomSummary>,
    pub(super) rooms_loaded: bool,
    pub(super) rooms_error: Option<String>,
    pub(super) refused: Option<String>,
    pub(super) rooms_at: Option<Instant>,
    pub(super) rooms_polling: bool,
    pub(super) digest: Option<String>,
    pub(super) digest_busy: bool,
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
    pub(super) faces: HashMap<String, Arc<Image>>,
    pub(super) faces_asked: HashSet<String>,
    pub(super) _inputs: [Subscription; 2],
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
            game: None,
            game_at: None,
            game_polling: false,
            rooms: Vec::new(),
            rooms_loaded: false,
            rooms_error: None,
            refused: None,
            rooms_at: None,
            rooms_polling: false,
            digest: None,
            digest_busy: false,
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
            faces: HashMap::new(),
            faces_asked: HashSet::new(),
            _inputs: inputs,
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.starting.is_some() || self.session.is_some()
    }

    pub(super) fn finish_leaving(&mut self) {
        self.session = None;
        self.launch_seen = 0;
        self.room_launching = false;
        self.state = None;
        self.leaving = false;
        self.closing = None;
        self.game_seen = false;
        self.viewing = None;
        self.rooms_at = None;
        self.game_at = None;
        self.digest = None;
    }
}

pub(super) fn due(at: Option<Instant>, every: Duration, now: Instant) -> bool {
    at.is_none_or(|at| now.duration_since(at) >= every)
}
