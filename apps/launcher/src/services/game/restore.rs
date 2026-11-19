use std::path::PathBuf;
use std::time::Duration;

use swofty_core::game_folder::recovery::{self, Scope, SessionLock};
use swofty_core::install::Game;
use swofty_core::platform::process::{game_processes, game_running_in};
use swofty_core::store::Store;
use swofty_i18n::{fill, tr, trf, trn};

use super::discovery::discover;

/// At launcher start: put back what an unfinished `swofty run` session left
/// in the game folder, unless a session is still live. A `swofty apply`
/// install stays.
pub fn recover_stale(store: &Store) -> Result<Vec<String>, String> {
    if !game_processes().is_empty() {
        return Ok(Vec::new());
    }
    let Some(install) = discover(store) else {
        return Ok(Vec::new());
    };
    let Some(lock) = SessionLock::try_acquire(&store.root).map_err(|e| e.to_string())? else {
        return Ok(Vec::new());
    };
    let g = Game::locate(&store.root, install.dir).map_err(|e| e.to_string())?;
    let r = recovery::recover(&lock, &store.root, &g, Scope::Sessions).map_err(|e| e.to_string())?;
    Ok(r.lines())
}

/// How long Restore game folder waits for a Play that still holds the game
/// folder: its build, or its start or roll back with F1 25 closed.
const RESTORE_WAIT: recovery::Wait = recovery::Wait {
    build: Duration::from_secs(15 * 60),
    other: Duration::from_secs(120),
};

/// The Restore game folder button: what `swofty restore` does, once a Play
/// that still holds the game folder lets go of it.
pub fn restore(store: &Store, dir: PathBuf) -> Result<String, String> {
    let g = Game::locate(&store.root, dir).map_err(|e| e.to_string())?;
    let running = || game_running_in(&g.dir);
    let lock = recovery::wait_for_lock(&store.root, RESTORE_WAIT, &running).map_err(|e| e.to_string())?;
    let helpers = store.helpers().pause();
    let done = recovery::restore_locked(&lock, &store.root, &g).map_err(|e| e.to_string())?;
    drop(helpers);
    drop(lock);
    if !done.left.is_empty() {
        let left: Vec<String> = done.left.iter().map(ToString::to_string).collect();
        return Err(trf!(
            "Not everything could be restored: {left}",
            left = left.join("; ")
        ));
    }
    Ok(match done.set_aside.len() {
        0 => tr("The game folder matches the original game files.").to_owned(),
        n => fill(
            &trn(
                n,
                "The game folder matches the original game files. {count} file was moved to {folder}.",
                "The game folder matches the original game files. {count} files were moved to {folder}.",
            ),
            &[(
                "folder",
                recovery::set_aside_dir(&store.root).display().to_string(),
            )],
        ),
    })
}
