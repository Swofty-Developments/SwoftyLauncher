use crate::dialogs::game_edited::Edited;
use crate::services::game::{self, Launch, Owner, Probe};
use crate::services::play_checks;
use crate::shell::notices::{Notification, Notify};
use gpui_kit::*;
use std::path::PathBuf;
use swofty_core::diagnostics::quarantine;
use swofty_core::game_folder::recovery::Blocked;
use swofty_core::install::Source;
use swofty_core::session::{LaunchMode, SessionRecord};
use swofty_core::store::Store;

use super::launch_alerts::note_notification;
use super::probe::GameWatch;
use super::{PlayState, Shell};
use swofty_i18n::{tr, trf, trn};

#[derive(Default)]
pub(crate) struct Launching {
    pub(crate) launch: Option<Launch>,
    pub(crate) notice: Option<String>,
    pub(crate) play_anyway: bool,
    pub(crate) play_pending: play_checks::Pending,
    pub(crate) restoring: bool,
    pub(crate) cancelling: bool,
    pub(crate) game_edited: Option<Edited>,
    pub(crate) steam_saves_once: bool,
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
                .map(|f| f.headline().to_owned());
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

impl Shell {
    pub(crate) fn play_state(&self) -> PlayState {
        self.launching
            .play_state(&self.game, self.addons.checking, self.crashes.dismissed.as_ref())
    }

    pub(crate) fn play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.play_for(None, window, cx);
    }

    /// Play, or Play for a room: armed to go straight to the room's lobby, with the host's row ids for the
    /// build when `room` has them. Whether Play was taken up: the pre-launch checks run off the UI thread
    /// and the launch follows when they pass.
    pub(crate) fn play_for(
        &mut self,
        room: Option<crate::features::multiplayer::RoomLaunch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.data_move_locked() || self.launching.play_pending.is_active() {
            return false;
        }
        let Some(install) = self.game.install.clone() else {
            return false;
        };
        if install.source == Source::EaApp && self.ea_app_warn_pending() {
            self.ea_app_warn(window, cx);
            return false;
        }
        if self.addons.checking {
            window.notify(
                Notification::info(tr("Play when the conflict check finishes.")),
                cx,
            );
            return false;
        }
        if let Some(moved) = install.moved.clone() {
            self.show_ea_moved(moved, window, cx);
            return false;
        }
        let Some(generation) = self.launching.play_pending.begin() else {
            return false;
        };
        let inputs = play_checks::Inputs {
            store: self.store.clone(),
            profile: self.library.profile.name.clone(),
            game: install.dir.clone(),
            build_id: install.build_id,
            anyway: std::mem::take(&mut self.launching.play_anyway),
        };
        // This press's ask only: a dialog, a cancel or a stale result must not hand it to the next Play.
        let steam_saves_once = std::mem::take(&mut self.launching.steam_saves_once);
        cx.notify();
        let is_room = room.is_some();
        cx.spawn_in(window, async move |this, cx| {
            let checked = cx
                .background_executor()
                .spawn(async move { play_checks::run(inputs) })
                .await;
            this.update_in(cx, |s, window, cx| {
                if !s.launching.play_pending.finish(generation) {
                    return;
                }
                let started = s.play_checked(checked, room, steam_saves_once, window, cx);
                if is_room {
                    s.mp_room_launch_result(started);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    pub(super) fn play_checked(
        &mut self,
        checked: play_checks::Checked,
        room: Option<crate::features::multiplayer::RoomLaunch>,
        steam_saves_once: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        crate::app::app_log::note(&checked.timings.line());
        let stale = self.library.profile.name != checked.profile
            || self.game.install.as_ref().map(|i| &i.dir) != Some(&checked.game);
        let Some(install) = self.game.install.clone().filter(|_| !stale) else {
            crate::app::app_log::note("[play] dropped: the profile or game copy changed during the checks");
            return false;
        };
        let gate = checked.gate;
        if gate.write_blocked {
            self.show_admin_needed(&install, window, cx);
            return false;
        }
        if !gate.setup_done {
            self.open_setup(window, cx);
            return false;
        }
        if !gate.offline_accepted {
            self.confirm_offline_play(window, cx);
            return false;
        }
        if gate.first_launch {
            self.first_launch_notice(window, cx);
            return false;
        }
        self.launching.notice = None;
        self.game.blocked = None;
        self.launching.game_edited = None;
        Blocked::clear(&self.store.root).ok();
        self.seen.blocked = game::blocked_at(&self.store.root).unwrap_or(0.0);
        self.crashes.dismissed = self.game.session.as_ref().map(|s| s.dir.clone());
        for note in &checked.notes {
            window.notify(note_notification(note), cx);
        }
        match checked.verdict {
            play_checks::Verdict::Go => {}
            play_checks::Verdict::Stop(stop) => {
                self.launching.notice = Some(stop);
                return false;
            }
            play_checks::Verdict::Confirm(text) => {
                self.confirm_missing_rows(text, window, cx);
                return false;
            }
        }
        if let Some(d) = swofty_core::diagnostics::known_problems::outdated_exe(install.version().as_deref())
        {
            window.notify(Notification::warning(d.text()).title(d.title).autohide(false), cx);
        }
        if let Some(short) = crate::features::settings::low_space_warning(checked.room) {
            let text = trf!(
                "{short} Settings shows what takes the room and clears old logs.",
                short = short
            );
            crate::app::app_log::note(&format!("play: {text}"));
            window.notify(
                Notification::warning(text)
                    .title(tr("Low on disk space"))
                    .autohide(false),
                cx,
            );
        }
        let armed = (room.is_some() || self.account.allowed()).then(|| {
            swofty_core::multiplayer::client::launch_env_value(&self.store.root, SESSION_MODULE_WAIT)
        });
        match Launch::start(
            &self.store,
            &self.library.profile.name,
            &install,
            self.launch_mode(),
            armed.as_deref(),
            room.as_ref().and_then(|r| r.row_ids.as_deref()),
            room.as_ref().map(|r| r.direct_start.as_str()),
            steam_saves_once,
        ) {
            Ok(l) => {
                self.launching.launch = Some(l);
                true
            }
            Err(e) => {
                self.seen.launch_failed = game::now_secs();
                window.notify(
                    Notification::error(e)
                        .title(tr("Could not launch F1 25"))
                        .autohide(false),
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn saves_refused(&self) -> Option<&SessionRecord> {
        self.game.session.as_ref().filter(|s| {
            s.exit_code == Some(swofty_core::saves::launcher::EXIT)
                && self.crashes.dismissed.as_ref() != Some(&s.dir)
                && s.ended.is_some_and(|e| game::now_secs() - e < 1800.0)
        })
    }

    pub(crate) fn play_with_steam_saves(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.launching.steam_saves_once = true;
        if !self.play_for(None, window, cx) {
            self.launching.steam_saves_once = false;
        }
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

    use super::Launching;
    use crate::services::game::{Copies, Install, Owner};
    use crate::shell::PlayState;
    use crate::shell::probe::GameWatch;

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
}
