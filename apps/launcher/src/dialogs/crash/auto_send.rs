//! Sending a crash's logs by themselves when F1 25 crashed in a room: the room each running game is in, kept in
//! its session folder, which crashes are sent without asking, and the send itself, once per crash.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use swofty_core::diagnostics::send_logs::{
    self, Automatic, Bundle, GameInfo, Outcome, Report, RoomRun, Stage, automatic,
};
use swofty_core::online::multiplayer::client;
use swofty_core::online::server::sign_in;
use swofty_core::session::{STOP_FILE, SessionRecord};
use swofty_core::store::Store;

/// Follows the running game's session and keeps the room it runs in written in its folder, so a crash is still
/// known to be a room's once the launcher has left the room, or after the launcher restarts.
#[derive(Default)]
pub(crate) struct RoomWatch {
    dir: Option<PathBuf>,
    run: Option<RoomRun>,
}

impl RoomWatch {
    /// One look: `running` is that F1 25 runs now, `room` the room the launcher is in now. Only a session whose
    /// game runs is followed, so the room a crashed game was in stays as it was when the game went down.
    pub(crate) fn follow(&mut self, session: Option<&SessionRecord>, running: bool, room: Option<RoomRun>) {
        let Some(s) = session.filter(|s| running && s.started.is_some() && s.ended.is_none()) else {
            return;
        };
        if self.dir.as_ref() != Some(&s.dir) {
            self.run = RoomRun::read(&s.dir);
            self.dir = Some(s.dir.clone());
        }
        let next = next_run(self.run.as_ref(), room);
        if next == self.run {
            return;
        }
        match &next {
            Some(run) => {
                if let Err(e) = run.write(&s.dir) {
                    eprintln!("swofty-launcher: could not note the room in the session folder: {e}");
                }
            }
            None => RoomRun::forget(&s.dir),
        }
        self.run = next;
    }

    /// A session that just ended: whether its game ran in a room. One that ended before any look saw its game
    /// run, as a game that goes down at once, takes the room the launcher is in now.
    pub(crate) fn ended(&mut self, session: &SessionRecord, live: Option<RoomRun>) -> bool {
        let followed = self.dir.as_ref() == Some(&session.dir) && self.run.is_some();
        if followed || RoomRun::read(&session.dir).is_some() {
            return true;
        }
        let Some(room) = live else {
            return false;
        };
        if let Err(e) = room.write(&session.dir) {
            eprintln!("swofty-launcher: could not note the room in the session folder: {e}");
        }
        self.dir = Some(session.dir.clone());
        self.run = Some(room);
        true
    }

    /// Names, in the room a session's game ran in, the step the room's `state` was at as the game went down, in
    /// place of the step the last look saw, when the state names one.
    pub(crate) fn at_step(&mut self, session: &SessionRecord, state: Option<&Value>) {
        let Some(step) = state.and_then(automatic::step_at) else {
            return;
        };
        let Some(mut run) = RoomRun::read(&session.dir) else {
            return;
        };
        if note_step(&session.dir, &mut run, step) && self.dir.as_ref() == Some(&session.dir) {
            self.run = Some(run);
        }
    }
}

/// Names `step` as the step of the room in the session folder `dir`, when the room names another; whether it did.
fn note_step(dir: &Path, run: &mut RoomRun, step: String) -> bool {
    if run.step.as_deref() == Some(step.as_str()) {
        return false;
    }
    run.step = Some(step);
    if let Err(e) = run.write(dir) {
        eprintln!("swofty-launcher: could not note the room in the session folder: {e}");
    }
    true
}

/// How long the automatic send waits for the room's state to record the game's exit, which the room's session writes
/// on its own thread after the launcher tells it.
const EXIT_WAIT: Duration = Duration::from_secs(2);

/// The room's state, by `look`, once it records the exit of the game that started at `since_ms`, with the step it
/// recorded that exit at; looked for every 100 ms until `until`. None at once when the launcher is in no room.
fn exit_state(since_ms: u64, until: Instant, look: impl Fn() -> Option<Value>) -> Option<(Value, String)> {
    loop {
        let state = look()?;
        if let Some(step) = automatic::exit_step(&state, since_ms) {
            return Some((state, step));
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// How a game that ran in a room ended, as the launcher tells the room's session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GameEnd {
    pub(crate) exit_code: Option<i32>,
    /// The launcher asked the session to stop: its Stop, or a room closing the game to start it again.
    pub(crate) stopped_by_launcher: bool,
    pub(crate) crashed: bool,
}

impl GameEnd {
    pub(crate) fn of(session: &SessionRecord) -> Self {
        let stopped_by_launcher = session.dir.join(STOP_FILE).exists();
        Self {
            exit_code: session
                .end
                .as_ref()
                .and_then(|e| e.exit_code)
                .or(session.exit_code),
            stopped_by_launcher,
            crashed: session.crashed() && !stopped_by_launcher,
        }
    }
}

/// The room a running game is in after one look: the room the launcher is in now, keeping that a room started
/// the game once one did; with no room now, the last one only when it started the game.
pub(crate) fn next_run(last: Option<&RoomRun>, now: Option<RoomRun>) -> Option<RoomRun> {
    match now {
        Some(mut now) => {
            now.launched |= last.is_some_and(|l| l.launched);
            Some(now)
        }
        None => last.filter(|l| l.launched).cloned(),
    }
}

/// What a finished session tells the crash screen about its logs, read from its folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CrashFacts {
    /// The session ended in a crash: not a stop, a quit or a close with no sign of a crash.
    pub(crate) crashed: bool,
    /// It ended recently enough for the crash screen to open by itself.
    pub(crate) fresh: bool,
    /// The launcher asked the session to stop: its Stop, or a room closing the game to start it again.
    pub(crate) stop_requested: bool,
    pub(crate) room: Option<RoomRun>,
    /// How an earlier automatic send for this crash went.
    pub(crate) outcome: Option<Outcome>,
}

impl CrashFacts {
    pub(crate) fn read(session: &SessionRecord, fresh: bool) -> Self {
        Self {
            crashed: session.crashed(),
            fresh,
            stop_requested: session.dir.join(STOP_FILE).exists(),
            room: RoomRun::read(&session.dir),
            outcome: Outcome::read(&session.dir),
        }
    }

    /// `allowed` is that the signed-in player has multiplayer access, without which the crash screen names no room
    /// and sends nothing by itself; `enabled` is the setting, `can_send` that this build has a log server to send to.
    pub(crate) fn plan(self, allowed: bool, enabled: bool, can_send: bool) -> Plan {
        if !allowed || !self.crashed || self.stop_requested {
            return Plan::Ask;
        }
        if let Some(outcome) = self.outcome {
            return Plan::Done(outcome);
        }
        match self.room {
            Some(room) if enabled && can_send && self.fresh => Plan::Send(room),
            _ => Plan::Ask,
        }
    }
}

/// What the crash screen does about the logs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Plan {
    /// Offers Send Logs, as for any crash.
    Ask,
    /// Sends them now, for a crash in this room.
    Send(RoomRun),
    /// Says how the send for this crash went, and never sends it again.
    Done(Outcome),
}

/// The automatic send for the crash on screen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AutoSend {
    Sending,
    Sent { reference: Option<String> },
    Failed { why: String },
}

impl AutoSend {
    /// The logs are sent or on their way, so the screen offers no second send.
    pub(crate) fn covers(&self) -> bool {
        !matches!(self, Self::Failed { .. })
    }
}

impl From<Outcome> for AutoSend {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Sent { reference, .. } => Self::Sent { reference },
            Outcome::Failed { why } => Self::Failed { why },
        }
    }
}

/// One automatic send: the crashed session, the room its game was in, and what the bundle reads.
pub(crate) struct Job {
    pub(crate) store: Store,
    pub(crate) session: PathBuf,
    pub(crate) game: GameInfo,
    pub(crate) room: RoomRun,
    /// The room's state, while the launcher is still in it.
    pub(crate) snapshot: Option<Value>,
    /// How far the send got, for the crash screen.
    pub(crate) stage: Arc<Mutex<Stage>>,
}

impl Job {
    /// Collects, keeps and sends the bundle; blocking, so off the UI thread. The session folder records the send
    /// as interrupted before it starts, so a launcher that closes halfway never sends the same crash twice.
    pub(crate) fn run(mut self) -> Outcome {
        if let Err(e) = Outcome::interrupted().write(&self.session) {
            eprintln!("swofty-launcher: could not note the automatic send in the session folder: {e}");
        }
        self.at_exit(Instant::now() + EXIT_WAIT, client::snapshot);
        let automatic = Automatic {
            session: folder_name(&self.session),
            room: Some(self.room),
        };
        let report = Report::automatic(sender(&self.store), self.game, self.snapshot, automatic);
        let progress = |s: Stage| {
            if let Ok(mut slot) = self.stage.lock() {
                *slot = s;
            }
        };
        let bundle = Bundle::collect(&self.store, &report);
        progress(Stage::Packing);
        if let Err(e) = bundle.save(&send_logs::kept_dir(&self.store), send_logs::KEPT) {
            eprintln!("swofty-launcher: could not keep the logs zip: {e}");
        }
        let outcome = match send_logs::send(&bundle, &progress) {
            Ok(sent) => Outcome::Sent {
                parts: sent.parts,
                reference: sent.reference,
            },
            Err(e) => Outcome::Failed { why: e.to_string() },
        };
        if let Err(e) = outcome.write(&self.session) {
            eprintln!("swofty-launcher: could not note the automatic send in the session folder: {e}");
        }
        outcome
    }

    /// Takes, once the room's state records this game's exit, that state for the bundle and the step it recorded the
    /// exit at for the room, in place of the step the state was at just before the exit.
    fn at_exit(&mut self, until: Instant, look: impl Fn() -> Option<Value>) {
        let Some(started) = SessionRecord::read(&self.session).started else {
            return;
        };
        let Some((state, step)) = exit_state((started * 1000.0) as u64, until, look) else {
            return;
        };
        note_step(&self.session, &mut self.room, step);
        self.snapshot = Some(state);
    }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The name an automatic bundle goes under: the Discord username Send Logs remembers, else the signed-in
/// account's name, which a room needs.
pub(crate) fn sender(store: &Store) -> String {
    sender_from(
        store.config().discord_username.as_deref(),
        sign_in::load(&store.root).map(|s| s.name).as_deref(),
    )
}

pub(crate) fn sender_from(remembered: Option<&str>, signed_in: Option<&str>) -> String {
    let usable = |name: &str| {
        let name: String = name.chars().filter(|c| !c.is_control() && *c != '`').collect();
        send_logs::check_name(&name).ok()
    };
    remembered
        .and_then(usable)
        .or_else(|| signed_in.and_then(usable))
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use serde_json::json;
    use swofty_core::diagnostics::send_logs::{GameInfo, Outcome, RoomRun, Stage};
    use swofty_core::session::{Ending, STOP_FILE, SessionEnd, SessionRecord};
    use swofty_core::store::Store;

    use super::{AutoSend, CrashFacts, GameEnd, Job, Plan, RoomWatch, next_run, sender_from};

    const ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;

    fn room(code: &str, launched: bool) -> RoomRun {
        RoomRun {
            code: Some(code.into()),
            step: Some("in_lobby".into()),
            phase: Some("racing".into()),
            host: false,
            launched,
        }
    }

    fn crash_in(room: Option<RoomRun>) -> CrashFacts {
        CrashFacts {
            crashed: true,
            fresh: true,
            stop_requested: false,
            room,
            outcome: None,
        }
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-auto-send-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ended(dir: &std::path::Path, end: Ending, exit_code: i32) -> SessionRecord {
        SessionRecord {
            dir: dir.to_path_buf(),
            request_id: None,
            started: Some(1.0),
            ended: Some(2.0),
            exit_code: Some(exit_code),
            end: Some(SessionEnd {
                end,
                exit_code: Some(exit_code),
                crash_code: None,
                reason: String::new(),
            }),
        }
    }

    fn running(dir: &std::path::Path) -> SessionRecord {
        SessionRecord {
            dir: dir.to_path_buf(),
            request_id: None,
            started: Some(1.0),
            ended: None,
            exit_code: None,
            end: None,
        }
    }

    #[test]
    fn a_crash_in_a_room_is_sent_at_once() {
        assert_eq!(
            crash_in(Some(room("K7Q2PX", true))).plan(true, true, true),
            Plan::Send(room("K7Q2PX", true))
        );
        assert_eq!(
            crash_in(Some(room("K7Q2PX", false))).plan(true, true, true),
            Plan::Send(room("K7Q2PX", false))
        );
    }

    #[test]
    fn a_crash_outside_a_room_asks_as_before() {
        assert_eq!(crash_in(None).plan(true, true, true), Plan::Ask);
    }

    #[test]
    fn a_stop_a_quit_or_a_room_closing_the_game_is_never_sent() {
        let not_a_crash = CrashFacts {
            crashed: false,
            ..crash_in(Some(room("K7Q2PX", true)))
        };
        assert_eq!(not_a_crash.plan(true, true, true), Plan::Ask);
        let stopped = CrashFacts {
            stop_requested: true,
            ..crash_in(Some(room("K7Q2PX", true)))
        };
        assert_eq!(stopped.plan(true, true, true), Plan::Ask);
    }

    #[test]
    fn the_setting_off_or_a_build_with_nowhere_to_send_asks_as_before() {
        assert_eq!(
            crash_in(Some(room("K7Q2PX", true))).plan(true, false, true),
            Plan::Ask
        );
        assert_eq!(
            crash_in(Some(room("K7Q2PX", true))).plan(true, true, false),
            Plan::Ask
        );
    }

    #[test]
    fn a_player_without_multiplayer_access_gets_the_plain_crash_screen() {
        assert_eq!(
            crash_in(Some(room("K7Q2PX", true))).plan(false, true, true),
            Plan::Ask
        );
        let sent = CrashFacts {
            outcome: Some(Outcome::Sent {
                parts: 1,
                reference: Some("1311204".into()),
            }),
            ..crash_in(Some(room("K7Q2PX", true)))
        };
        assert_eq!(
            sent.plan(false, true, true),
            Plan::Ask,
            "an earlier send is not shown either"
        );
    }

    #[test]
    fn an_old_crash_found_at_start_is_not_sent_late() {
        let old = CrashFacts {
            fresh: false,
            ..crash_in(Some(room("K7Q2PX", true)))
        };
        assert_eq!(old.plan(true, true, true), Plan::Ask);
    }

    #[test]
    fn a_crash_already_sent_or_tried_is_never_sent_again() {
        let sent = Outcome::Sent {
            parts: 1,
            reference: Some("1311204".into()),
        };
        let again = CrashFacts {
            outcome: Some(sent.clone()),
            ..crash_in(Some(room("K7Q2PX", true)))
        };
        assert_eq!(again.clone().plan(true, true, true), Plan::Done(sent.clone()));
        assert_eq!(again.plan(true, false, false), Plan::Done(sent));
        let tried = CrashFacts {
            outcome: Some(Outcome::interrupted()),
            ..crash_in(Some(room("K7Q2PX", true)))
        };
        assert_eq!(tried.plan(true, true, true), Plan::Done(Outcome::interrupted()));
    }

    #[test]
    fn a_failed_send_gives_the_button_back() {
        assert!(AutoSend::Sending.covers());
        assert!(
            AutoSend::from(Outcome::Sent {
                parts: 2,
                reference: None
            })
            .covers()
        );
        let failed = AutoSend::from(Outcome::Failed {
            why: "Couldn't reach the log server: offline".into(),
        });
        assert!(!failed.covers());
        assert_eq!(
            failed,
            AutoSend::Failed {
                why: "Couldn't reach the log server: offline".into()
            }
        );
    }

    #[test]
    fn the_facts_come_from_the_session_folder() {
        let dir = folder("facts");
        let crash = ended(&dir, Ending::Crash, ACCESS_VIOLATION);
        assert_eq!(CrashFacts::read(&crash, true), crash_in(None));
        room("K7Q2PX", true).write(&dir).unwrap();
        assert_eq!(
            CrashFacts::read(&crash, true).plan(true, true, true),
            Plan::Send(room("K7Q2PX", true))
        );
        Outcome::interrupted().write(&dir).unwrap();
        assert_eq!(
            CrashFacts::read(&crash, true).plan(true, true, true),
            Plan::Done(Outcome::interrupted())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_launcher_stop_that_ends_in_terminate_process_1_is_not_sent() {
        let dir = folder("stopped");
        room("K7Q2PX", true).write(&dir).unwrap();
        std::fs::write(dir.join(STOP_FILE), "12.5\n").unwrap();
        let stopped = ended(&dir, Ending::Stopped, 1);
        assert_eq!(CrashFacts::read(&stopped, true).plan(true, true, true), Plan::Ask);
        let crash_after_stop = ended(&dir, Ending::Crash, ACCESS_VIOLATION);
        assert_eq!(
            CrashFacts::read(&crash_after_stop, true).plan(true, true, true),
            Plan::Ask
        );
        std::fs::remove_file(dir.join(STOP_FILE)).unwrap();
        let closed = ended(&dir, Ending::Unknown, 1);
        assert_eq!(CrashFacts::read(&closed, true).plan(true, true, true), Plan::Ask);
        let quit = ended(&dir, Ending::Quit, 0);
        assert_eq!(CrashFacts::read(&quit, true).plan(true, true, true), Plan::Ask);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_room_that_started_the_game_stays_with_it_after_the_room_is_left() {
        assert_eq!(next_run(None, None), None);
        assert_eq!(next_run(None, Some(room("A", false))), Some(room("A", false)));
        assert_eq!(next_run(Some(&room("A", false)), None), None);
        assert_eq!(next_run(Some(&room("A", true)), None), Some(room("A", true)));
        assert_eq!(
            next_run(Some(&room("A", true)), Some(room("B", false))),
            Some(room("B", true))
        );
    }

    #[test]
    fn the_watch_writes_the_room_while_the_game_runs_and_keeps_it_when_it_goes_down() {
        let dir = folder("watch");
        let mut watch = RoomWatch::default();
        watch.follow(Some(&running(&dir)), true, Some(room("K7Q2PX", false)));
        assert_eq!(RoomRun::read(&dir), Some(room("K7Q2PX", false)));
        watch.follow(Some(&running(&dir)), true, Some(room("K7Q2PX", true)));
        assert_eq!(RoomRun::read(&dir), Some(room("K7Q2PX", true)));
        watch.follow(Some(&running(&dir)), false, None);
        assert_eq!(RoomRun::read(&dir), Some(room("K7Q2PX", true)));
        let crash = ended(&dir, Ending::Crash, ACCESS_VIOLATION);
        watch.follow(Some(&crash), true, None);
        assert_eq!(RoomRun::read(&dir), Some(room("K7Q2PX", true)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_watch_forgets_a_room_left_while_a_game_it_did_not_start_runs_on() {
        let dir = folder("left");
        let mut watch = RoomWatch::default();
        watch.follow(Some(&running(&dir)), true, Some(room("K7Q2PX", false)));
        assert!(RoomRun::read(&dir).is_some());
        watch.follow(Some(&running(&dir)), true, None);
        assert_eq!(RoomRun::read(&dir), None);
        let mut restarted = RoomWatch::default();
        room("K7Q2PX", true).write(&dir).unwrap();
        restarted.follow(Some(&running(&dir)), true, None);
        assert_eq!(RoomRun::read(&dir), Some(room("K7Q2PX", true)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_automatic_bundle_goes_under_the_remembered_name_else_the_accounts() {
        assert_eq!(sender_from(Some("swofty"), Some("Swofty Dev")), "swofty");
        assert_eq!(sender_from(None, Some("Swofty Dev")), "Swofty Dev");
        assert_eq!(sender_from(Some("x"), Some("max`\nv")), "maxv");
        assert_eq!(sender_from(None, None), "unknown");
    }

    #[test]
    fn a_session_that_just_ended_ran_in_the_room_its_folder_or_the_launcher_names() {
        let dir = folder("ended");
        let crash = ended(&dir, Ending::Crash, ACCESS_VIOLATION);
        let mut watch = RoomWatch::default();
        assert!(!watch.ended(&crash, None));
        assert_eq!(RoomRun::read(&dir), None);
        assert!(watch.ended(&crash, Some(room("K7Q2PX", false))));
        assert_eq!(RoomRun::read(&dir), Some(room("K7Q2PX", false)));
        assert!(RoomWatch::default().ended(&crash, None));
        let mut followed = RoomWatch::default();
        let other = folder("ended-followed");
        followed.follow(Some(&running(&other)), true, Some(room("A", true)));
        assert!(followed.ended(
            &ended(&other, Ending::Crash, ACCESS_VIOLATION),
            Some(room("B", false))
        ));
        assert_eq!(RoomRun::read(&other), Some(room("A", true)));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&other).ok();
    }

    #[test]
    fn the_room_notes_the_step_its_state_was_at_when_the_game_went_down() {
        let dir = folder("at-step");
        let crash = ended(&dir, Ending::Crash, ACCESS_VIOLATION);
        let state = json!({"steps": [
            {"id": "account", "state": "done"},
            {"id": "lobby", "state": "done"},
            {"id": "race", "state": "active"},
        ]});
        let mut watch = RoomWatch::default();
        watch.at_step(&crash, Some(&state));
        assert_eq!(RoomRun::read(&dir), None, "a session with no room gets none");
        watch.follow(Some(&running(&dir)), true, Some(room("K7Q2PX", true)));
        watch.at_step(&crash, None);
        watch.at_step(&crash, Some(&json!({"phase": {"phase": "racing"}})));
        assert_eq!(
            RoomRun::read(&dir),
            Some(room("K7Q2PX", true)),
            "a state without steps keeps the step the last look saw"
        );
        watch.at_step(&crash, Some(&state));
        let at_race = RoomRun {
            step: Some("race".into()),
            ..room("K7Q2PX", true)
        };
        assert_eq!(RoomRun::read(&dir), Some(at_race.clone()));
        assert!(watch.ended(&crash, None));
        assert_eq!(
            CrashFacts::read(&crash, true).plan(true, true, true),
            Plan::Send(at_race)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_send_takes_the_step_the_room_recorded_for_this_games_exit() {
        let dir = folder("at-exit");
        std::fs::write(dir.join("game_start"), "100.5").unwrap();
        let at_race = RoomRun {
            step: Some("race".into()),
            ..room("K7Q2PX", true)
        };
        at_race.write(&dir).unwrap();
        let mut job = Job {
            store: Store::new(dir.join("data")),
            session: dir.clone(),
            game: GameInfo::default(),
            room: at_race.clone(),
            snapshot: None,
            stage: Arc::new(Mutex::new(Stage::Collecting)),
        };
        let state = |at_ms: u64| json!({"steps": [], "game_exit": {"code": "game_crashed", "step": "lobby", "at_ms": at_ms}});
        let now = Instant::now();
        job.at_exit(now, || None);
        assert_eq!(
            (&job.room, &job.snapshot),
            (&at_race, &None),
            "no room, nothing to wait for"
        );
        job.at_exit(now, || Some(state(100_499)));
        assert_eq!(
            (&job.room, &job.snapshot),
            (&at_race, &None),
            "an exit from before the game started"
        );
        let looks = Cell::new(0);
        job.at_exit(now + Duration::from_secs(10), || {
            looks.set(looks.get() + 1);
            Some(state(if looks.get() < 3 { 100_499 } else { 100_500 }))
        });
        assert_eq!(looks.get(), 3, "the send waits for the room to record the exit");
        let at_lobby = RoomRun {
            step: Some("lobby".into()),
            ..at_race
        };
        assert_eq!(job.room, at_lobby);
        assert_eq!(RoomRun::read(&dir), Some(at_lobby));
        assert_eq!(job.snapshot, Some(state(100_500)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_room_hears_a_crash_a_stop_and_a_quit_as_the_crash_screen_judges_them() {
        let dir = folder("game-end");
        assert_eq!(
            GameEnd::of(&ended(&dir, Ending::Crash, ACCESS_VIOLATION)),
            GameEnd {
                exit_code: Some(ACCESS_VIOLATION),
                stopped_by_launcher: false,
                crashed: true,
            }
        );
        assert_eq!(
            GameEnd::of(&ended(&dir, Ending::Quit, 0)),
            GameEnd {
                exit_code: Some(0),
                stopped_by_launcher: false,
                crashed: false,
            }
        );
        std::fs::write(dir.join(STOP_FILE), "12.5\n").unwrap();
        assert_eq!(
            GameEnd::of(&ended(&dir, Ending::Crash, ACCESS_VIOLATION)),
            GameEnd {
                exit_code: Some(ACCESS_VIOLATION),
                stopped_by_launcher: true,
                crashed: false,
            }
        );
        assert_eq!(
            GameEnd::of(&ended(&dir, Ending::Stopped, 1)),
            GameEnd {
                exit_code: Some(1),
                stopped_by_launcher: true,
                crashed: false,
            }
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
