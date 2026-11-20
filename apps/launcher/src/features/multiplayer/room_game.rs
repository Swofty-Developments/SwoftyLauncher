use std::path::Path;
use std::time::Instant;

use swofty_core::online::multiplayer::client::Session;
use swofty_core::online::multiplayer::types::LaunchAsk;

use super::model::exit_leaves;
use super::state::{EXIT_WAIT, RoomLaunch};

/// What the room's game asks of the room's session.
pub(crate) trait RoomSession {
    fn launch_ask(&self) -> Option<LaunchAsk>;
    fn launched(&self);
    fn launch_failed(&self, why: &str);
}

impl RoomSession for Session {
    fn launch_ask(&self) -> Option<LaunchAsk> {
        self.state().launch
    }

    fn launched(&self) {
        Session::launched(self);
    }

    fn launch_failed(&self, why: &str) {
        Session::launch_failed(self, why);
    }
}

/// The room's game: starting it when the session asks, and following it until it exits.
#[derive(Default)]
pub(crate) struct RoomGame {
    /// The game processes the room closed before it started; the watch waits for them to go.
    pub(super) closing: Option<Vec<u32>>,
    /// The game the room started has been seen running.
    pub(super) seen: bool,
    /// The last launch ask taken up.
    pub(super) ask_seq: u32,
    /// A launch the room started is under way.
    pub(super) launching: bool,
    /// A prompt Play opened holds the room launch: its Play goes on with it through `Shell::play`.
    pub(super) held: Option<RoomLaunch>,
    /// When the room's game was first seen gone, while the room waits for the session to write how it ended.
    pub(super) exited_at: Option<Instant>,
}

impl RoomGame {
    /// The session's launch ask when the game should start for it: a new ask, while no launch, build,
    /// restore or game is running.
    pub(crate) fn due(&self, session: &impl RoomSession, busy: bool) -> Option<LaunchAsk> {
        let ask = session.launch_ask()?;
        (self.ask_seq != ask.seq && !busy).then_some(ask)
    }

    /// Takes up `ask`: writes the host's row ids for the build and arms the room launch. None when the ids
    /// could not be written, which the session is told.
    pub(crate) fn begin(
        &mut self,
        session: &impl RoomSession,
        ask: LaunchAsk,
        root: &Path,
    ) -> Option<RoomLaunch> {
        self.closing = None;
        let row_ids = match &ask.row_ids {
            Some(ids) => {
                let file = root.join("state/room-row-ids.json");
                if let Err(e) = std::fs::create_dir_all(root.join("state"))
                    .map_err(|e| e.to_string())
                    .and_then(|()| ids.write(&file).map_err(|e| e.to_string()))
                {
                    self.ask_seq = ask.seq;
                    session.launch_failed(&format!("the host's ids and settings couldn't be written ({e})"));
                    return None;
                }
                Some(file)
            }
            None => None,
        };
        self.ask_seq = ask.seq;
        self.seen = false;
        self.launching = true;
        Some(RoomLaunch {
            row_ids,
            direct_start: ask.direct_start,
        })
    }

    pub(crate) fn hold(&mut self, room: RoomLaunch) {
        self.held = Some(room);
    }

    pub(crate) fn take_held(&mut self) -> Option<RoomLaunch> {
        self.held.take()
    }

    /// Drops a held room launch once its prompt closed without Play. True when one was dropped.
    pub(crate) fn drop_held(&mut self, session: Option<&impl RoomSession>) -> bool {
        if self.held.take().is_none() {
            return false;
        }
        self.launching = false;
        if let Some(session) = session {
            session.launch_failed("Play was cancelled");
        }
        true
    }

    pub(crate) fn launch_result(&mut self, session: Option<&impl RoomSession>, started: bool) {
        let Some(session) = session else {
            return;
        };
        if started {
            session.launched();
        } else {
            self.launching = false;
            session.launch_failed("see the notice, finish what it asks, then try again");
        }
    }

    /// The launch `begin` armed ended without the game: `why` says what stopped it.
    pub(crate) fn launch_ended(&mut self, session: Option<&impl RoomSession>, why: Option<String>) {
        if !std::mem::take(&mut self.launching) {
            return;
        }
        match why {
            None => self.seen = true,
            Some(why) => {
                if let Some(session) = session {
                    session.launch_failed(&why);
                }
            }
        }
    }

    /// Follows the game. True once the game the room started has exited and nothing is starting it again.
    pub(crate) fn exited(&mut self, running: bool, busy: bool) -> bool {
        if running && self.ask_seq != 0 && self.closing.is_none() {
            self.seen = true;
        }
        if self.closing.is_some() && !running {
            self.closing = None;
        }
        let gone = exit_leaves(self.seen, running, busy);
        if !gone {
            self.exited_at = None;
        }
        gone
    }

    /// Whether the room may leave for its exited game: once the game's session wrote how it ended
    /// (`end.json`), so the crash watch tells the session first, or [`EXIT_WAIT`] after the exit was first
    /// seen without that.
    pub(crate) fn exit_settled(&mut self, end_written: impl FnOnce() -> bool, now: Instant) -> bool {
        let since = *self.exited_at.get_or_insert(now);
        end_written() || now.duration_since(since) >= EXIT_WAIT
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use swofty_core::online::multiplayer::types::LaunchAsk;
    use swofty_core::row_ids::Overrides;

    use super::{RoomGame, RoomSession};
    use crate::features::multiplayer::RoomLaunch;

    #[derive(Default)]
    struct Told {
        ask: Option<LaunchAsk>,
        said: RefCell<Vec<String>>,
    }

    impl Told {
        fn asking(seq: u32, row_ids: Option<Overrides>) -> Self {
            Self {
                ask: Some(LaunchAsk {
                    seq,
                    row_ids,
                    id_addons: 0,
                    setting_addons: 0,
                    restart: false,
                    why: String::new(),
                    direct_start: "lobby".into(),
                }),
                ..Self::default()
            }
        }

        fn said(&self) -> Vec<String> {
            self.said.borrow().clone()
        }
    }

    impl RoomSession for Told {
        fn launch_ask(&self) -> Option<LaunchAsk> {
            self.ask.clone()
        }

        fn launched(&self) {
            self.said.borrow_mut().push("launched".into());
        }

        fn launch_failed(&self, why: &str) {
            self.said.borrow_mut().push(format!("failed: {why}"));
        }
    }

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("swofty-room-game-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn held() -> RoomLaunch {
        RoomLaunch {
            row_ids: None,
            direct_start: "lobby".into(),
        }
    }

    #[test]
    fn a_new_ask_is_taken_up_once_and_only_while_the_game_is_free() {
        let mut game = RoomGame {
            closing: Some(vec![7]),
            seen: true,
            ..RoomGame::default()
        };
        assert!(game.due(&Told::default(), false).is_none());
        let session = Told::asking(3, None);
        assert!(game.due(&session, true).is_none());
        let ask = game.due(&session, false).unwrap();
        let room = game.begin(&session, ask, Path::new("unused")).unwrap();
        assert_eq!(room.row_ids, None);
        assert_eq!(room.direct_start, "lobby");
        assert_eq!(game.ask_seq, 3);
        assert!(game.launching);
        assert!(!game.seen);
        assert!(game.closing.is_none());
        assert!(game.due(&session, false).is_none());
        assert!(game.due(&Told::asking(4, None), false).is_some());
        assert!(session.said().is_empty());
    }

    #[test]
    fn the_hosts_row_ids_are_written_for_the_build() {
        let root = root("row-ids");
        let session = Told::asking(1, Some(Overrides::default()));
        let mut game = RoomGame::default();
        let ask = game.due(&session, false).unwrap();
        let room = game.begin(&session, ask, &root).unwrap();
        let file = root.join("state/room-row-ids.json");
        assert_eq!(room.row_ids.as_deref(), Some(file.as_path()));
        assert!(file.is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn row_ids_that_cannot_be_written_fail_that_ask_once() {
        let root = root("row-ids-blocked");
        std::fs::write(root.join("state"), "a file where the folder goes").unwrap();
        let session = Told::asking(2, Some(Overrides::default()));
        let mut game = RoomGame::default();
        let ask = game.due(&session, false).unwrap();
        assert!(game.begin(&session, ask, &root).is_none());
        let said = session.said();
        assert_eq!(said.len(), 1);
        assert!(said[0].starts_with("failed: the host's ids and settings couldn't be written ("));
        assert!(!game.launching);
        assert!(game.due(&session, false).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_held_launch_dropped_tells_the_room_play_was_cancelled() {
        let session = Told::default();
        let mut game = RoomGame::default();
        assert!(!game.drop_held(Some(&session)));
        game.hold(held());
        assert!(game.take_held().is_some());
        assert!(!game.drop_held(Some(&session)));
        game.launching = true;
        game.hold(held());
        assert!(game.drop_held(Some(&session)));
        assert!(!game.launching);
        assert!(game.held.is_none());
        assert_eq!(session.said(), ["failed: Play was cancelled"]);
        game.hold(held());
        assert!(game.drop_held(None::<&Told>));
    }

    #[test]
    fn a_launch_result_tells_the_room_whether_the_game_started() {
        let session = Told::default();
        let mut game = RoomGame {
            launching: true,
            ..RoomGame::default()
        };
        game.launch_result(Some(&session), true);
        assert!(game.launching);
        game.launch_result(Some(&session), false);
        assert!(!game.launching);
        assert_eq!(
            session.said(),
            [
                "launched",
                "failed: see the notice, finish what it asks, then try again"
            ]
        );
        game.launching = true;
        game.launch_result(None::<&Told>, false);
        assert!(game.launching);
    }

    #[test]
    fn a_launch_that_ended_says_why_or_counts_the_game_as_seen() {
        let session = Told::default();
        let mut game = RoomGame::default();
        game.launch_ended(Some(&session), Some("not this one".into()));
        assert!(session.said().is_empty());
        game.launching = true;
        game.launch_ended(Some(&session), Some("the build failed".into()));
        assert!(!game.launching);
        assert!(!game.seen);
        assert_eq!(session.said(), ["failed: the build failed"]);
        game.launching = true;
        game.launch_ended(Some(&session), None);
        assert!(!game.launching);
        assert!(game.seen);
        assert_eq!(session.said().len(), 1);
    }

    #[test]
    fn the_room_is_left_once_the_game_it_started_exits() {
        let mut game = RoomGame::default();
        assert!(!game.exited(true, false));
        assert!(!game.seen);
        assert!(!game.exited(false, false));
        game.ask_seq = 1;
        assert!(!game.exited(true, false));
        assert!(game.seen);
        assert!(!game.exited(false, true));
        assert!(game.exited(false, false));
    }

    #[test]
    fn a_game_the_room_closed_is_not_the_rooms_game() {
        let mut game = RoomGame {
            ask_seq: 1,
            closing: Some(vec![7]),
            ..RoomGame::default()
        };
        assert!(!game.exited(true, false));
        assert!(!game.seen);
        assert!(game.closing.is_some());
        assert!(!game.exited(false, false));
        assert!(game.closing.is_none());
        assert!(!game.exited(true, false) && game.seen);
    }

    #[test]
    fn the_room_waits_for_the_games_end_to_be_written_before_it_leaves() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut game = RoomGame {
            ask_seq: 1,
            seen: true,
            ..RoomGame::default()
        };
        assert!(game.exited(false, false));
        assert!(!game.exit_settled(|| false, t0));
        assert!(!game.exit_settled(|| false, t0 + Duration::from_secs(4)));
        assert!(
            game.exit_settled(|| true, t0 + Duration::from_secs(1)),
            "the end is written"
        );
        assert!(
            game.exit_settled(|| false, t0 + super::EXIT_WAIT),
            "no end within the wait"
        );

        assert!(!game.exited(true, false), "the game runs again");
        assert!(game.exited_at.is_none());
        let t1 = t0 + Duration::from_secs(60);
        assert!(game.exited(false, false));
        assert!(
            !game.exit_settled(|| false, t1),
            "a later exit waits from when it was seen"
        );
    }
}
