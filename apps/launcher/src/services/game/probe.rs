use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use swofty_core::diagnostics::launch;
use swofty_core::diagnostics::quarantine;
use swofty_core::game_folder::busy::Busy;
use swofty_core::game_folder::recovery::{Blocked, Notice, SessionLock};
use swofty_core::game_folder::stock;
use swofty_core::online::server::integrity;
use swofty_core::platform::process::{GameProcess, game_processes};
use swofty_core::platform::steam;
use swofty_core::session::SessionRecord;
use swofty_core::store::Store;

use super::Copies;
use super::discovery::discover_all;
use super::import_retry::retry_import;
use super::restore::recover_stale;
use super::vault_watch::watch_vault;

pub struct Probe {
    pub install: Option<Copies>,
    pub processes: Vec<GameProcess>,
    pub session: Option<SessionRecord>,
    pub requests: Option<SystemTime>,
    pub merged: bool,
    pub recovered: Option<Notice>,
    pub launch_stage: Option<String>,
    pub mods_blocked: Option<quarantine::Report>,
    pub not_stock: Option<stock::Report>,
    pub blocked: Option<Blocked>,
    pub blocked_at: Option<f64>,
    pub launch_failed: Option<launch::Failure>,
    pub integrity_missing: Option<integrity::Notice>,
    pub busy: Option<Busy>,
    pub(crate) banner_profile: String,
    pub(crate) banner: Option<Option<crate::features::career_saves::Banner>>,
    pub(crate) saves_marker: bool,
    pub(crate) saves_print: Option<u64>,
}

pub fn probe(
    store: &Store,
    rediscover: bool,
    seen: Option<SystemTime>,
    profile: &str,
    banner_due: bool,
    saves_open: bool,
) -> Probe {
    let processes = game_processes();
    watch_vault(store, &processes);
    retry_import(store, &processes);
    let (requests, merged) = merge_requests(store, processes.is_empty(), seen);
    let saves_print = saves_open
        .then(|| crate::features::career_saves::saves_print(&store.root, profile, !processes.is_empty()));
    Probe {
        install: rediscover.then(|| discover_all(store)),
        processes,
        session: store.latest_session().map(|d| SessionRecord::read(&d)),
        requests,
        merged,
        recovered: Notice::read(&store.root),
        mods_blocked: quarantine::Report::read(&store.root),
        not_stock: stock::Report::read(&store.root),
        blocked: Blocked::read(&store.root),
        blocked_at: blocked_at(&store.root),
        launch_failed: launch::Failure::read(&store.root),
        integrity_missing: integrity::Notice::read(&store.root),
        busy: build_state(store),
        banner_profile: profile.to_owned(),
        banner: banner_due
            .then(|| crate::features::career_saves::SavesPage::banner_for(&store.root, profile)),
        saves_marker: swofty_core::saves::launcher::marker(&store.root).is_some(),
        saves_print,
        launch_stage: std::fs::read_to_string(store.root.join(steam::client::STAGE_FILE))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
    }
}

/// The build a Play is running. One whose process ended is rolled back
/// here, as at launcher start.
pub(super) fn build_state(store: &Store) -> Option<Busy> {
    Busy::read(&store.root)?;
    let live = Busy::live(&store.root);
    if live.is_none() {
        match recover_stale(store) {
            Ok(lines) => lines.iter().for_each(|l| eprintln!("swofty-launcher: {l}")),
            Err(e) => eprintln!("swofty-launcher: could not clean up an unfinished build: {e}"),
        }
        if let Ok(Some(_lock)) = SessionLock::try_acquire(&store.root) {
            Busy::clear(&store.root);
        }
    }
    live
}

pub fn blocked_at(data: &Path) -> Option<f64> {
    std::fs::metadata(Blocked::path(data))
        .and_then(|m| m.modified())
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

pub(super) fn merge_requests(
    store: &Store,
    idle: bool,
    seen: Option<SystemTime>,
) -> (Option<SystemTime>, bool) {
    let profile = store.active_profile();
    let Some(written) = std::fs::metadata(store.requests_path(&profile))
        .and_then(|m| m.modified())
        .ok()
    else {
        return (None, false);
    };
    let settled = idle && written.elapsed().is_ok_and(|age| age > Duration::from_secs(10));
    if seen == Some(written) && !settled {
        return (seen, false);
    }
    let merged = store.merge_requests(&profile, settled).ok().flatten();
    (Some(written), merged.is_some_and(|m| !m.changed.is_empty()))
}
