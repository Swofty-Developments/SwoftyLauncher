use std::sync::atomic::{AtomicBool, Ordering};

use swofty_core::game_folder::busy::Busy;
use swofty_core::game_folder::recovery::SessionLock;
use swofty_core::platform::process::{GameProcess, game_processes};
use swofty_core::saves::vault;
use swofty_core::store::Store;

use super::import_retry::import_first_start;

pub(super) static VAULT_WATCH: std::sync::LazyLock<std::sync::Mutex<vault::watch::Watch>> =
    std::sync::LazyLock::new(Default::default);

/// The career vault's snapshot of an F1 25 the launcher didn't start, and
/// again once it closes.
pub(super) fn watch_vault(store: &Store, processes: &[GameProcess]) {
    let pid = processes.first().map(|p| p.pid);
    let session = || SessionLock::held_elsewhere(&store.root) || Busy::live(&store.root).is_some();
    let lines = match VAULT_WATCH.lock() {
        Ok(mut w) => w.tick(&store.root, pid, session),
        Err(_) => Vec::new(),
    };
    if !lines.is_empty() {
        BANNER_STALE.store(true, Ordering::Relaxed);
    }
    lines.iter().for_each(|l| crate::app::app_log::note(l));
}

pub(super) static BANNER_STALE: AtomicBool = AtomicBool::new(false);

pub(crate) fn banner_stale() -> bool {
    BANNER_STALE.swap(false, Ordering::Relaxed)
}

/// At launcher start, once an unfinished session is rolled back: move old
/// save backups into the career vault and snapshot the save, unless F1 25
/// runs (the probe's watch takes that one) or a session holds the lock.
pub fn vault_at_start(store: &Store) {
    if !game_processes().is_empty() {
        if swofty_core::saves::launcher::marker(&store.root).is_none() {
            crate::app::app_log::note(
                "saves: F1 25 is running; the import of the Steam save into the launcher saves waits for the next launcher start",
            );
        }
        return;
    }
    let Ok(Some(_lock)) = SessionLock::try_acquire(&store.root) else {
        return;
    };
    for l in vault::watch::at_launcher_start(&store.root) {
        crate::app::app_log::note(&l);
    }
    let game = vault::Location::active(&store.root);
    for l in
        swofty_core::saves::stall::sweep(store, game.as_ref(), vault::restore::Running::now(), vault::now())
    {
        crate::app::app_log::note(&l);
    }
    for l in swofty_core::saves::stuck::sweep(store, vault::now()) {
        crate::app::app_log::note(&l);
    }
    for l in swofty_core::saves::plain::split_all_held(&store.root) {
        crate::app::app_log::note(&l);
    }
    for l in swofty_core::saves::plain::withdraw_all_unanswered(&store.root) {
        crate::app::app_log::note(&l);
    }
    for l in swofty_core::saves::plain::prune_all_held(&store.root) {
        crate::app::app_log::note(&l);
    }
    import_first_start(store);
    BANNER_STALE.store(true, Ordering::Relaxed);
}
