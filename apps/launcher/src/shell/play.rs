use crate::dialogs::game_edited::Edited;
use crate::features::multiplayer::RoomLaunch;
use crate::services::game::{self, Install, Launch, Owner, Probe};
use crate::services::play_checks;
use crate::shell::notices::{Notification, Notify};
use gpui_kit::*;
use std::path::PathBuf;
use swofty_core::addons::files::Damaged;
use swofty_core::diagnostics::quarantine;
use swofty_core::game_folder::recovery::Blocked;
use swofty_core::install::Source;
use swofty_core::platform::ea_app::Moved;
use swofty_core::session::{LaunchMode, SessionRecord};
use swofty_core::store::Store;

use super::host::{Host, Hosted, PromptHost, Prompted};
use super::launch_alerts::note_notification;
use super::probe::GameWatch;
use super::{PlayState, Seen, Shell};
use swofty_i18n::{tr, trf, trn};

/// The profile a launch plays: the room's when the room names one, else the active one.
pub(crate) fn played_profile<'a>(room: Option<&'a RoomLaunch>, active: &'a str) -> &'a str {
    room.and_then(|r| r.profile.as_deref()).unwrap_or(active)
}

#[derive(Default)]
pub(crate) struct Launching {
    pub(crate) launch: Option<Launch>,
    pub(crate) notice: Option<String>,
    pub(crate) play_pending: play_checks::Pending,
    pub(crate) restoring: bool,
    pub(crate) cancelling: bool,
    pub(crate) game_edited: Option<Edited>,
    pub(crate) steam_saves_once: bool,
}

/// What a Play press came to once its checks passed or stopped it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Played {
    Started,
    Stopped,
    /// A prompt is open whose Play presses Play again; a room launch waits on it.
    Asking,
}

impl Launching {
    /// Follows a running launch through one probe. Once the launch is over it is cleared and the
    /// result says why it failed, or `None` inside when the game started.
    pub(crate) fn follow(
        &mut self,
        game: &mut GameWatch,
        store: &Store,
        probe: &Probe,
    ) -> Option<Option<String>> {
        let launch = self.launch.as_mut()?;
        launch.saw_game |= game.owners.iter().any(|o| !matches!(o, Owner::Other(_)));
        launch.stage = probe.launch_stage.clone();
        let ok = launch.finished()?;
        let failed = if !ok && !launch.saw_game {
            game.blocked = Blocked::read(&store.root);
            let refused = probe
                .mods_blocked
                .as_ref()
                .is_some_and(|r| r.stage == quarantine::Stage::BeforeLaunch && r.at >= launch.at);
            let not_stock = probe
                .not_stock
                .as_ref()
                .filter(|r| r.at >= launch.at)
                .map(|r| crate::dialogs::game_edited::notice(r, game.install.as_ref().map(|i| i.source)));
            let failed = probe
                .launch_failed
                .as_ref()
                .filter(|f| f.at >= launch.at)
                .map(super::launch_alerts::launch_headline);
            self.notice = Some(match &game.blocked {
                _ if refused => tr("an antivirus removed mod files, so F1 25 was not started").to_owned(),
                _ if not_stock.is_some() => not_stock.unwrap_or_default(),
                Some(b) if b.ours() => trn(
                    b.files.len(),
                    "a file an earlier Swofty session left in the game folder is in the way; Restore game \
                     folder clears it",
                    "{count} files an earlier Swofty session left in the game folder are in the way; \
                     Restore game folder clears them",
                ),
                Some(b) => trn(
                    b.files.len(),
                    "a file Swofty did not put in the game folder is in the way",
                    "{count} files Swofty did not put in the game folder are in the way",
                ),
                None => failed.unwrap_or_else(|| launch.failure()),
            })
            .filter(|n| !n.contains(swofty_core::game_folder::busy::CANCELLED));
            let why = self.notice.clone().unwrap_or_else(|| launch.failure());
            if self.notice.is_some() {
                crate::app::app_log::action("start F1 25").failed(&why);
            }
            Some(why)
        } else {
            None
        };
        self.launch = None;
        Some(failed)
    }

    /// The HUD customisation warning of the build this launch made, once the game shows up, unless the
    /// Play press already showed it.
    pub(crate) fn hud_editor_warning(&mut self, store: &Store) -> Option<String> {
        let launch = self.launch.as_mut().filter(|l| l.saw_game && !l.hud_checked)?;
        launch.hud_checked = true;
        play_checks::fresh_hud_editor(
            swofty_core::profile::conflicts::last(store, &launch.profile),
            launch.at,
        )
    }

    /// Clears a pending cancel once the build it waited for is gone. True when it just settled.
    pub(crate) fn cancel_settled(&mut self, was_building: bool, still_busy: bool) -> bool {
        let settled = self.cancelling && !still_busy && was_building;
        if settled {
            self.cancelling = false;
        }
        settled
    }

    pub(crate) fn play_state(
        &self,
        game: &GameWatch,
        checking_addons: bool,
        dismissed_crash: Option<&PathBuf>,
    ) -> PlayState {
        if game.owners.contains(&Owner::Active) {
            return PlayState::Running;
        }
        if game.build.is_some() {
            return PlayState::Building;
        }
        if self.play_pending.is_active() {
            return PlayState::Preparing;
        }
        if let Some(launch) = &self.launch {
            return if launch.saw_game {
                PlayState::Restoring
            } else {
                PlayState::Launching
            };
        }
        if self.restoring {
            return PlayState::Restoring;
        }
        if !game.detected {
            return PlayState::Detecting;
        }
        if let Some(owner) = game.owners.first() {
            return PlayState::Elsewhere(owner.clone());
        }
        if game.install.is_none() {
            return PlayState::NotInstalled;
        }
        if checking_addons {
            return PlayState::Checking;
        }
        match &game.session {
            Some(s)
                if s.crashed()
                    && dismissed_crash != Some(&s.dir)
                    && s.ended.is_some_and(|e| game::now_secs() - e < 1800.0) =>
            {
                PlayState::Crashed(s.clone())
            }
            _ => PlayState::Ready,
        }
    }
}

/// How long Play waits for a session module load that is already running.
const SESSION_MODULE_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);

/// What a press of Play comes to before the checks run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Press {
    /// The data folder is moving, a press is already being checked, or there is no game copy.
    Ignored,
    EaAppWarn,
    WaitForConflicts,
    EaMoved(Moved),
    Check(Install),
}

impl Press {
    pub(crate) fn of(
        busy: bool,
        install: Option<&Install>,
        checking_addons: bool,
        ea_app_warn_due: impl FnOnce() -> bool,
    ) -> Self {
        let Some(install) = install.filter(|_| !busy) else {
            return Self::Ignored;
        };
        if install.source == Source::EaApp && ea_app_warn_due() {
            return Self::EaAppWarn;
        }
        if checking_addons {
            return Self::WaitForConflicts;
        }
        match &install.moved {
            Some(moved) => Self::EaMoved(moved.clone()),
            None => Self::Check(install.clone()),
        }
    }
}

/// What the Play checks' result comes to, in the order Play takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// The profile or the game copy changed while the checks ran.
    Dropped,
    AdminNeeded,
    Setup,
    ConfirmOffline,
    FirstLaunch,
    Stop(String),
    Launch,
}

impl Step {
    pub(crate) fn of(stale: bool, gate: play_checks::Gate, verdict: play_checks::Verdict) -> Self {
        if stale {
            return Self::Dropped;
        }
        if gate.write_blocked {
            return Self::AdminNeeded;
        }
        if !gate.setup_done {
            return Self::Setup;
        }
        if !gate.offline_accepted {
            return Self::ConfirmOffline;
        }
        if gate.first_launch {
            return Self::FirstLaunch;
        }
        match verdict {
            play_checks::Verdict::Go => Self::Launch,
            play_checks::Verdict::Stop(stop) => Self::Stop(stop),
        }
    }

    /// What Play came to when this step ends it; none for a launch, which says once it starts.
    pub(crate) fn played(&self) -> Option<Played> {
        match self {
            Self::Dropped | Self::AdminNeeded | Self::Setup | Self::Stop(_) => Some(Played::Stopped),
            Self::ConfirmOffline | Self::FirstLaunch => Some(Played::Asking),
            Self::Launch => None,
        }
    }

    /// Past the gate: the last launch's notice, blocked files and crash are cleared and the checks'
    /// notes shown.
    pub(crate) fn clears(&self) -> bool {
        matches!(self, Self::Stop(_) | Self::Launch)
    }
}

/// What Play opens in the window.
pub(crate) enum Prompt {
    EaAppWarn,
    EaMoved(Moved),
    AdminNeeded(Install),
    Setup,
    ConfirmOffline,
    FirstLaunch,
    Notice(Notification),
    Damaged(Damaged),
}

pub(crate) struct PlayNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) profile: &'a str,
    pub(crate) game: &'a mut GameWatch,
    pub(crate) seen: &'a mut Seen,
    /// The session whose crash the play bar no longer shows.
    pub(crate) dismissed: &'a mut Option<PathBuf>,
    /// The data folder is moving; Play waits until it is done.
    pub(crate) locked: bool,
    /// The conflict check is running; Play waits for it.
    pub(crate) checking_addons: bool,
    /// The signed-in account may join sessions, so the game starts ready for one.
    pub(crate) online: bool,
    pub(crate) mode: LaunchMode,
}

pub(crate) enum PlayChange {
    /// A room's Play passed its checks and started the game, or stopped.
    RoomLaunched(bool),
    /// A prompt whose Play presses Play again holds the room launch until it is answered.
    HoldRoom(RoomLaunch),
}

impl Hosted for Launching {
    type Needs<'a> = PlayNeeds<'a>;
    type Change = PlayChange;
}

impl Prompted for Launching {
    type Prompt = Prompt;
}

impl Host<Launching> for Shell {
    fn split(&mut self) -> (&mut Launching, PlayNeeds<'_>) {
        let (locked, online, mode) = (
            self.data_move_locked(),
            self.account.allowed(),
            self.launch_mode(),
        );
        (
            &mut self.launching,
            PlayNeeds {
                store: &self.store,
                profile: &self.library.profile.name,
                game: &mut self.game,
                seen: &mut self.seen,
                dismissed: &mut self.crashes.dismissed,
                locked,
                checking_addons: self.addons.conflicts.checking,
                online,
                mode,
            },
        )
    }

    fn changed(&mut self, change: PlayChange, _: &mut Context<Self>) {
        match change {
            PlayChange::RoomLaunched(started) => self.rooms.launch_result(started),
            PlayChange::HoldRoom(room) => self.rooms.game.hold(room),
        }
    }
}

impl PromptHost<Launching> for Shell {
    fn prompt(&mut self, prompt: Prompt, window: &mut Window, cx: &mut Context<Self>) {
        match prompt {
            Prompt::EaAppWarn => self.ea_app_warn(window, cx),
            Prompt::EaMoved(moved) => self.show_ea_moved(moved, window, cx),
            Prompt::AdminNeeded(install) => self.show_admin_needed(&install, window, cx),
            Prompt::Setup => self.open_setup(window, cx),
            Prompt::ConfirmOffline => self.confirm_offline_play(window, cx),
            Prompt::FirstLaunch => self.first_launch_notice(window, cx),
            Prompt::Notice(note) => window.notify(note, cx),
            Prompt::Damaged(d) => {
                let note = self.damaged_notification(&d, cx);
                window.notify(note, cx);
            }
        }
    }
}

impl Launching {
    /// Play, or Play for a room: armed to go straight to the room's lobby, with the host's row ids for the
    /// build when `room` has them. Whether Play was taken up: the pre-launch checks run off the UI thread
    /// and the launch follows when they pass.
    pub(crate) fn play_for<H: PromptHost<Launching>>(
        host: &mut H,
        room: Option<RoomLaunch>,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> bool {
        let (launching, needs) = host.split();
        let busy = needs.locked || launching.play_pending.is_active();
        let store = needs.store;
        let press = Press::of(busy, needs.game.install.as_ref(), needs.checking_addons, || {
            crate::dialogs::ea_app::warn_due(store)
        });
        let install = match press {
            Press::Ignored => return false,
            Press::EaAppWarn => {
                host.prompt(Prompt::EaAppWarn, window, cx);
                return false;
            }
            Press::WaitForConflicts => {
                let wait = Notification::info(tr("Play when the conflict check finishes."));
                host.prompt(Prompt::Notice(wait), window, cx);
                return false;
            }
            Press::EaMoved(moved) => {
                host.prompt(Prompt::EaMoved(moved), window, cx);
                return false;
            }
            Press::Check(install) => install,
        };
        let (launching, needs) = host.split();
        let Some(generation) = launching.play_pending.begin() else {
            return false;
        };
        let profile = played_profile(room.as_ref(), needs.profile);
        let inputs = play_checks::Inputs {
            store: needs.store.clone(),
            profile: profile.to_owned(),
            game: install.dir.clone(),
            build_id: install.build_id,
        };
        // This press's ask only: a dialog, a cancel or a stale result must not hand it to the next Play.
        let steam_saves_once = std::mem::take(&mut launching.steam_saves_once);
        cx.notify();
        let is_room = room.is_some();
        cx.spawn_in(window, async move |this, cx| {
            let checked = cx
                .background_executor()
                .spawn(async move { play_checks::run(inputs) })
                .await;
            this.update_in(cx, |host, window, cx| {
                if !host.split().0.play_pending.finish(generation) {
                    return;
                }
                let played = Self::play_checked(host, checked, room, steam_saves_once, window, cx);
                if is_room && played != Played::Asking {
                    host.changed(PlayChange::RoomLaunched(played == Played::Started), cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    fn play_checked<H: PromptHost<Launching>>(
        host: &mut H,
        checked: play_checks::Checked,
        room: Option<RoomLaunch>,
        steam_saves_once: bool,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> Played {
        crate::app::app_log::note(&checked.timings.line());
        for line in &checked.lines {
            crate::app::app_log::note(line);
        }
        let (launching, needs) = host.split();
        let hud_noted = checked.hud_noted;
        let stale = played_profile(room.as_ref(), needs.profile) != checked.profile
            || needs.game.install.as_ref().map(|i| &i.dir) != Some(&checked.game);
        let step = Step::of(stale, checked.gate, checked.verdict);
        let Some(install) = needs.game.install.clone().filter(|_| step != Step::Dropped) else {
            crate::app::app_log::note("[play] dropped: the profile or game copy changed during the checks");
            return Played::Stopped;
        };
        let (notes, damaged) = if step.clears() {
            launching.notice = None;
            launching.game_edited = None;
            needs.game.blocked = None;
            Blocked::clear(&needs.store.root).ok();
            needs.seen.blocked = game::blocked_at(&needs.store.root).unwrap_or(0.0);
            *needs.dismissed = needs.game.session.as_ref().map(|s| s.dir.clone());
            (checked.notes, checked.damaged)
        } else {
            (Vec::new(), Vec::new())
        };
        let played = step.played();
        let prompt = match step {
            Step::AdminNeeded => Some(Prompt::AdminNeeded(install.clone())),
            Step::Setup => Some(Prompt::Setup),
            Step::ConfirmOffline => Some(Prompt::ConfirmOffline),
            Step::FirstLaunch => Some(Prompt::FirstLaunch),
            Step::Stop(stop) => {
                crate::app::app_log::scrubbed(&format!("[play] stopped: {stop}"));
                launching.notice = Some(stop);
                None
            }
            Step::Dropped | Step::Launch => None,
        };
        for note in &notes {
            host.prompt(Prompt::Notice(note_notification(note)), window, cx);
        }
        for d in damaged.into_iter().filter(Damaged::stops_play) {
            host.prompt(Prompt::Damaged(d), window, cx);
        }
        if let Some(played) = played {
            if let Some(prompt) = prompt {
                host.prompt(prompt, window, cx);
            }
            if played == Played::Asking
                && let Some(room) = room
            {
                host.changed(PlayChange::HoldRoom(room), cx);
            }
            return played;
        }
        if let Some(d) = swofty_core::diagnostics::known_problems::outdated_exe(install.version().as_deref())
        {
            let outdated = Notification::warning(d.text()).title(d.title).autohide(false);
            host.prompt(Prompt::Notice(outdated), window, cx);
        }
        if let Some(short) = crate::features::settings::low_space_warning(checked.room) {
            let text = trf!(
                "{short} Settings shows what takes the room and clears old logs.",
                short = short
            );
            crate::app::app_log::note(&format!("play: {text}"));
            let low = Notification::warning(text)
                .title(tr("Low on disk space"))
                .autohide(false);
            host.prompt(Prompt::Notice(low), window, cx);
        }
        let (launching, needs) = host.split();
        let armed = (room.is_some() || needs.online).then(|| {
            swofty_core::online::multiplayer::client::launch_env_value(&needs.store.root, SESSION_MODULE_WAIT)
        });
        match Launch::start(
            needs.store,
            played_profile(room.as_ref(), needs.profile),
            &install,
            needs.mode,
            armed.as_deref(),
            room.as_ref().and_then(|r| r.row_ids.as_deref()),
            room.as_ref().map(|r| r.direct_start.as_str()),
            steam_saves_once,
        ) {
            Ok(mut l) => {
                l.hud_checked = hud_noted;
                launching.launch = Some(l);
                Played::Started
            }
            Err(e) => {
                needs.seen.launch_failed = game::now_secs();
                let failed = Notification::error(e)
                    .title(tr("Could not launch F1 25"))
                    .autohide(false);
                host.prompt(Prompt::Notice(failed), window, cx);
                Played::Stopped
            }
        }
    }

    pub(crate) fn play_with_steam_saves<H: PromptHost<Launching>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) {
        host.split().0.steam_saves_once = true;
        if !Self::play_for(host, None, window, cx) {
            host.split().0.steam_saves_once = false;
        }
    }
}

impl Shell {
    /// What Play shows for an addon with damaged files that stop it, with a Reinstall button
    /// when the Addon Browser offers it.
    fn damaged_notification(&self, d: &Damaged, cx: &mut Context<Self>) -> Notification {
        let offered = self.browser.find(&d.id).is_some_and(|a| {
            a.offered(&swofty_core::updates::release::current_version())
                .is_some()
        });
        let body = if offered {
            trf!(
                "{files}. Reinstall it to put its files back.",
                files = d.summary()
            )
        } else {
            trf!(
                "{files}. Install it again from the file you installed it from.",
                files = d.summary()
            )
        };
        let note = Notification::error(body)
            .title(trf!("{name} is damaged", name = d.name))
            .autohide(false);
        if !offered {
            return note;
        }
        let (shell, id) = (cx.entity().downgrade(), d.id.clone());
        note.action(tr("Reinstall"), move |_, cx| {
            shell
                .update(cx, |s, cx| crate::features::browser::Browser::install(s, &id, cx))
                .ok();
        })
    }

    pub(crate) fn play_state(&self) -> PlayState {
        self.launching.play_state(
            &self.game,
            self.addons.conflicts.checking,
            self.crashes.dismissed.as_ref(),
        )
    }

    pub(crate) fn play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let room = self.rooms.game.take_held();
        let is_room = room.is_some();
        if !Launching::play_for(self, room, window, cx) && is_room {
            self.rooms.launch_result(false);
        }
    }

    pub(crate) fn saves_refused(&self) -> Option<&SessionRecord> {
        self.game.session.as_ref().filter(|s| {
            s.exit_code == Some(swofty_core::saves::launcher::EXIT)
                && self.crashes.dismissed.as_ref() != Some(&s.dir)
                && s.ended.is_some_and(|e| game::now_secs() - e < 1800.0)
        })
    }

    pub(crate) fn launch_mode(&self) -> LaunchMode {
        if self.prefs.vr_profiles.contains(&self.library.profile.name) {
            LaunchMode::Vr
        } else {
            LaunchMode::Normal
        }
    }

    /// The game switcher: play another copy of F1 25.
    pub(crate) fn use_copy(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.launching.play_pending.cancel();
        let mut config = self.store.config();
        swofty_core::game_folder::copies::set_active(&mut config, &dir);
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
            return;
        }
        if let Some(copy) = self.game.copies.iter().find(|c| c.dir == dir) {
            self.game.install = Some(copy.clone());
            crate::app::app_log::set_game(Some(&copy.dir));
        }
        self.game.attribute();
        self.launching.notice = None;
        self.game.rediscover = true;
        self.reload(cx);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use swofty_core::install::Source;

    use super::{Launching, Played, Press, Step};

    #[test]
    fn a_room_launch_plays_the_room_profile_and_others_the_active_one() {
        use crate::features::multiplayer::RoomLaunch;
        let room = RoomLaunch {
            row_ids: None,
            profile: Some("-room".into()),
            direct_start: "lobby".into(),
        };
        assert_eq!(super::played_profile(Some(&room), "mine"), "-room");
        let hosting = RoomLaunch {
            profile: None,
            ..room
        };
        assert_eq!(super::played_profile(Some(&hosting), "mine"), "mine");
        assert_eq!(super::played_profile(None, "mine"), "mine");
    }
    use crate::services::game::{Copies, Install, Owner};
    use crate::services::play_checks::{Gate, Verdict};
    use crate::shell::PlayState;
    use crate::shell::probe::GameWatch;
    use swofty_core::platform::ea_app::Moved;

    fn install(dir: &str) -> Install {
        Install {
            dir: PathBuf::from(dir),
            build_id: None,
            exe_version: None,
            mode: String::new(),
            via_steam: true,
            manual: false,
            source: Source::Steam,
            moved: None,
        }
    }

    fn found(active: &str) -> Copies {
        Copies {
            all: vec![install(active)],
            active: Some(install(active)),
            missing: Vec::new(),
        }
    }

    #[test]
    fn copies_count_as_changed_until_first_seen_and_when_play_moves() {
        let mut game = GameWatch::new();
        assert!(game.take_copies(found("a")));
        assert!(game.detected);
        assert!(!game.take_copies(found("a")));
        assert!(game.take_copies(found("b")));
        assert_eq!(game.install, Some(install("b")));
    }

    #[test]
    fn play_waits_for_detection_then_needs_an_install() {
        let launching = Launching::default();
        let mut game = GameWatch::new();
        assert!(matches!(
            launching.play_state(&game, false, None),
            PlayState::Detecting
        ));
        game.detected = true;
        assert!(matches!(
            launching.play_state(&game, false, None),
            PlayState::NotInstalled
        ));
        game.take_copies(found("a"));
        assert!(matches!(
            launching.play_state(&game, false, None),
            PlayState::Ready
        ));
        assert!(matches!(
            launching.play_state(&game, true, None),
            PlayState::Checking
        ));
    }

    #[test]
    fn a_running_game_outranks_a_build_and_a_build_outranks_restoring() {
        let mut launching = Launching::default();
        let mut game = GameWatch::new();
        game.take_copies(found("a"));
        launching.restoring = true;
        assert!(matches!(
            launching.play_state(&game, false, None),
            PlayState::Restoring
        ));
        game.build = Some(Default::default());
        assert!(matches!(
            launching.play_state(&game, false, None),
            PlayState::Building
        ));
        game.owners = vec![Owner::Active];
        assert!(matches!(
            launching.play_state(&game, false, None),
            PlayState::Running
        ));
    }

    #[test]
    fn a_cancel_settles_once_the_build_is_gone() {
        let mut launching = Launching {
            cancelling: true,
            ..Default::default()
        };
        assert!(!launching.cancel_settled(true, true));
        assert!(launching.cancelling);
        assert!(launching.cancel_settled(true, false));
        assert!(!launching.cancelling);
        assert!(!launching.cancel_settled(true, false));
    }

    const PASSED: Gate = Gate {
        write_blocked: false,
        setup_done: true,
        offline_accepted: true,
        first_launch: false,
    };

    #[test]
    fn checks_that_pass_launch_the_game() {
        let step = Step::of(false, PASSED, Verdict::Go);
        assert_eq!(step, Step::Launch);
        assert_eq!(step.played(), None);
        assert!(step.clears());
    }

    #[test]
    fn a_profile_or_copy_changed_during_the_checks_drops_the_press() {
        let blocked = Gate {
            write_blocked: true,
            ..PASSED
        };
        let step = Step::of(true, blocked, Verdict::Go);
        assert_eq!(step, Step::Dropped);
        assert_eq!(step.played(), Some(Played::Stopped));
        assert!(!step.clears());
    }

    #[test]
    fn a_folder_it_cannot_write_asks_for_admin_before_anything_else() {
        let gate = Gate {
            write_blocked: true,
            setup_done: false,
            offline_accepted: false,
            first_launch: true,
        };
        let step = Step::of(false, gate, Verdict::Stop("no".into()));
        assert_eq!(step, Step::AdminNeeded);
        assert_eq!(step.played(), Some(Played::Stopped));
        assert!(!step.clears());
    }

    #[test]
    fn unfinished_setup_opens_setup_and_stops() {
        let gate = Gate {
            setup_done: false,
            offline_accepted: false,
            ..PASSED
        };
        let step = Step::of(false, gate, Verdict::Go);
        assert_eq!(step, Step::Setup);
        assert_eq!(step.played(), Some(Played::Stopped));
        assert!(!step.clears());
    }

    #[test]
    fn the_offline_terms_and_the_first_launch_notice_ask_and_hold_a_room() {
        let offline = Gate {
            offline_accepted: false,
            first_launch: true,
            ..PASSED
        };
        assert_eq!(Step::of(false, offline, Verdict::Go), Step::ConfirmOffline);
        let first = Gate {
            first_launch: true,
            ..PASSED
        };
        assert_eq!(Step::of(false, first, Verdict::Go), Step::FirstLaunch);
        for step in [Step::ConfirmOffline, Step::FirstLaunch] {
            assert_eq!(step.played(), Some(Played::Asking));
            assert!(!step.clears());
        }
    }

    #[test]
    fn a_stop_verdict_shows_its_reason_on_the_play_bar() {
        let step = Step::of(false, PASSED, Verdict::Stop("a mod is broken".into()));
        assert_eq!(step, Step::Stop("a mod is broken".into()));
        assert_eq!(step.played(), Some(Played::Stopped));
        assert!(step.clears());
    }

    fn ea_app(moved: Option<Moved>) -> Install {
        Install {
            source: Source::EaApp,
            moved,
            ..install("ea")
        }
    }

    #[test]
    fn a_press_while_busy_or_without_a_game_copy_is_ignored() {
        let steam = install("a");
        assert_eq!(Press::of(true, Some(&steam), false, || true), Press::Ignored);
        assert_eq!(Press::of(false, None, false, || true), Press::Ignored);
        assert_eq!(
            Press::of(false, Some(&steam), false, || true),
            Press::Check(steam)
        );
    }

    #[test]
    fn an_ea_app_copy_warns_once_before_the_checks() {
        let ea = ea_app(None);
        assert_eq!(Press::of(false, Some(&ea), true, || true), Press::EaAppWarn);
        assert_eq!(Press::of(false, Some(&ea), false, || false), Press::Check(ea));
    }

    #[test]
    fn a_running_conflict_check_holds_the_press() {
        let steam = install("a");
        assert_eq!(
            Press::of(false, Some(&steam), true, || true),
            Press::WaitForConflicts
        );
    }

    #[test]
    fn a_moved_ea_app_copy_shows_where_it_went() {
        let moved = Moved {
            dir: PathBuf::from("new"),
            recorded: PathBuf::from("old"),
            there: false,
        };
        let ea = ea_app(Some(moved.clone()));
        assert_eq!(
            Press::of(false, Some(&ea), false, || false),
            Press::EaMoved(moved)
        );
    }
}
