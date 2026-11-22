use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use swofty_core::catalog;
use swofty_core::online::multiplayer::client::Local;
use swofty_core::online::multiplayer::matching::{self, Own, Progress, ROOM_PROFILE};
use swofty_core::online::multiplayer::types::{HostPrint, SessionState, StepProblem, StepState};
use swofty_core::store::Store;
use swofty_i18n::{tr, trf};

use super::model::{blocker_line, replace_lines};
use super::state::read_local;

/// Matching the host's addons for a room launch: the Addon Browser installs, then the room profile, for the
/// host's fingerprint `for_digest`. The room launch waits until it is done.
#[derive(Default)]
pub(crate) struct RoomFix {
    pub(super) for_digest: Option<String>,
    pub(super) running: Option<Arc<Mutex<Progress>>>,
    pub(super) failed: Option<String>,
    pub(super) done: bool,
    pub(super) finished_at: Option<Instant>,
}

/// How long the room launch waits after the match for the session to take the room profile's fingerprint, so
/// its launch ask lends the host's ids for the addons just installed.
pub(crate) const SETTLE: Duration = Duration::from_secs(1);

/// What the addons step shows while the launcher matches the host's addons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FixView {
    Running(Progress),
    Failed(String),
}

impl RoomFix {
    /// Whether the room launch waits at `now`: the host's addons are in and the room profile isn't built for
    /// them, or was built less than [`SETTLE`] ago.
    pub(crate) fn holds(&self, host: Option<&HostPrint>, now: Instant) -> bool {
        let settled = self
            .finished_at
            .is_some_and(|at| now.saturating_duration_since(at) >= SETTLE);
        host.is_some_and(|h| !(self.done && settled && self.for_digest.as_deref() == Some(h.digest.as_str())))
    }

    /// Whether a match should start for `host`: one isn't running or failed, it isn't for these addons
    /// already, and the room's game isn't starting or running (`launched`).
    pub(crate) fn due(&self, host: Option<&HostPrint>, launched: bool) -> bool {
        host.is_some_and(|h| {
            self.running.is_none()
                && self.failed.is_none()
                && !launched
                && self.for_digest.as_deref() != Some(h.digest.as_str())
        })
    }

    pub(crate) fn begin(&mut self, digest: &str) -> Arc<Mutex<Progress>> {
        let progress = Arc::new(Mutex::new(Progress::default()));
        self.for_digest = Some(digest.to_owned());
        self.running = Some(progress.clone());
        self.failed = None;
        self.done = false;
        self.finished_at = None;
        progress
    }

    pub(crate) fn finish(&mut self, result: Result<(), String>, now: Instant) {
        self.running = None;
        self.finished_at = Some(now);
        match result {
            Ok(()) => self.done = true,
            Err(why) => self.failed = Some(why),
        }
    }

    /// Clears a failure so the match runs again. True when there was one.
    pub(crate) fn retry(&mut self) -> bool {
        if self.failed.take().is_none() {
            return false;
        }
        self.for_digest = None;
        true
    }

    pub(crate) fn view(&self) -> Option<FixView> {
        if let Some(progress) = &self.running {
            let now = progress.lock().map(|p| p.clone()).unwrap_or_default();
            return Some(FixView::Running(now));
        }
        self.failed.clone().map(FixView::Failed)
    }
}

const MB: f64 = 1024.0 * 1024.0;

pub(crate) fn progress_line(p: &Progress) -> String {
    if p.total == 0 {
        return tr("Turning on the host's addons").to_owned();
    }
    let at = match &p.replacing {
        Some(mine) => trf!(
            "Installs {name} {version}, replacing your {mine}",
            name = p.name,
            version = p.version,
            mine = mine
        ),
        None => trf!("Installing {name} {version}", name = p.name, version = p.version),
    };
    trf!(
        "{at}: {done} of {total} addons, {mb} of {total_mb} MB",
        at = at,
        done = p.done.min(p.total),
        total = p.total,
        mb = format!("{:.0}", p.bytes as f64 / MB),
        total_mb = format!("{:.0}", p.total_bytes as f64 / MB)
    )
}

/// The session's addons step as the launcher's match makes it: active with its progress, or failed with
/// Retry.
pub(crate) fn patch(state: &mut SessionState, fix: &FixView) {
    let Some(step) = state.steps.iter_mut().find(|s| s.id == "addons") else {
        return;
    };
    step.warning = false;
    match fix {
        FixView::Running(p) => {
            step.state = StepState::Active;
            step.problem = None;
            step.detail = Some(progress_line(p));
        }
        FixView::Failed(why) => {
            step.state = StepState::Failed;
            step.detail = None;
            step.problem = Some(StepProblem {
                code: "addons_install_failed".into(),
                text: trf!("Couldn't match the host's addons: {why}", why = why),
                detail: None,
                retry: Some(true),
                fix_plan: Vec::new(),
                warning: false,
            });
        }
    }
}

/// Installs what the host's addons need from the Addon Browser, builds the room profile and reads its
/// fingerprint, off the UI thread. `own_profile` gives only the player's cosmetic addons.
pub(crate) fn run(
    store: &Store,
    own_profile: &str,
    dir: &Path,
    host: &HostPrint,
    progress: &Mutex<Progress>,
) -> Result<(Local, Vec<String>), String> {
    let latest = catalog::fetch(&store.root)
        .ok()
        .or_else(|| catalog::cached(&store.root))
        .ok_or_else(|| tr("the Addon Browser can't be reached").to_owned())?;
    let game =
        swofty_core::install::Game::locate(&store.root, dir.to_path_buf()).map_err(|e| e.to_string())?;
    let mut own = Own::read(store, Some(game.identity.exe_sha256.clone())).map_err(|e| e.to_string())?;
    own.catalog = latest.clone();
    let plan = matching::plan_for(host, &own);
    if let Some(blocker) = plan.blockers.first() {
        return Err(blocker_line(blocker));
    }
    let launcher = swofty_core::updates::release::current_version();
    matching::install_all(
        &plan,
        &latest,
        &launcher,
        &|addon, v, bytes| matching::install_listed(store, &latest, addon, v, bytes),
        &|p| {
            if let Ok(mut now) = progress.lock() {
                *now = p.clone();
            }
        },
    )?;
    matching::write_room_profile(store, &host.addons, &host.options, own_profile)
        .map_err(|e| e.to_string())?;
    let local = read_local(store, ROOM_PROFILE, dir)?;
    Ok((local, replace_lines(&plan)))
}
