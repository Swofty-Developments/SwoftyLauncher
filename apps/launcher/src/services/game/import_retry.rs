use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use swofty_core::game_folder::busy::Busy;
use swofty_core::game_folder::copies::{self};
use swofty_core::game_folder::recovery::SessionLock;
use swofty_core::install::Source;
use swofty_core::platform::process::GameProcess;
use swofty_core::platform::steam;
use swofty_core::saves::vault;
use swofty_core::store::Store;

pub(super) static IMPORT_MISS_LOGGED: AtomicBool = AtomicBool::new(false);

pub(super) fn import_first_start(store: &Store) {
    let profiles = store.profile_names().unwrap_or_default();
    let running = vault::restore::Running::now();
    let log_miss = !IMPORT_MISS_LOGGED.swap(true, Ordering::Relaxed);
    log_import(store, &profiles, running, log_miss);
}

pub(crate) fn import_now(store: &Store) {
    let profiles = store.profile_names().unwrap_or_default();
    log_import(store, &profiles, vault::restore::Running::now(), true);
}

pub(super) fn log_import(
    store: &Store,
    profiles: &[String],
    running: vault::restore::Running,
    log_miss: bool,
) {
    for l in swofty_core::saves::launcher::first_start_import(
        &store.root,
        profiles,
        running,
        vault::now(),
        log_miss,
    ) {
        crate::app::app_log::note(&l);
    }
}

pub(super) static IMPORT_RETRY: AtomicI64 = AtomicI64::new(0);
pub(super) const IMPORT_RETRY_SECS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RetryGate {
    pub(super) game_running: bool,
    pub(super) pending: bool,
    pub(super) launch_in_progress: bool,
    pub(super) steam_usable: bool,
    pub(super) ea_copy: bool,
    pub(super) now: i64,
    pub(super) last: i64,
}

pub(super) fn should_retry_import(g: RetryGate) -> bool {
    !g.game_running
        && g.pending
        && !g.launch_in_progress
        && !g.ea_copy
        && g.steam_usable
        && g.now - g.last >= IMPORT_RETRY_SECS
}

pub(super) fn launch_in_progress(store: &Store) -> bool {
    Busy::live(&store.root).is_some()
        || SessionLock::held_elsewhere(&store.root)
        || std::fs::read_to_string(store.root.join(steam::client::STAGE_FILE))
            .is_ok_and(|s| !s.trim().is_empty())
}

/// While the first-start import has found no Steam save, try again once Steam is signed in.
pub(super) fn retry_import(store: &Store, processes: &[GameProcess]) {
    if !processes.is_empty()
        || !swofty_core::saves::launcher::import_pending(&store.root)
        || launch_in_progress(store)
    {
        return;
    }
    let now = vault::now();
    let last = IMPORT_RETRY.load(Ordering::Relaxed);
    if now - last < IMPORT_RETRY_SECS {
        return;
    }
    let gate = RetryGate {
        game_running: false,
        pending: true,
        launch_in_progress: false,
        steam_usable: swofty_core::platform::steam::client::status().state.usable(),
        ea_copy: copies::find_active(&store.config()).is_some_and(|c| c.source == Source::EaApp),
        now,
        last,
    };
    if !should_retry_import(gate) {
        return;
    }
    IMPORT_RETRY.store(now, Ordering::Relaxed);
    let Ok(Some(_lock)) = SessionLock::try_acquire(&store.root) else {
        return;
    };
    import_first_start(store);
}
