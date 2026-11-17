use std::path::PathBuf;

use swofty_core::game_folder::recovery::{self, Scope, SessionLock};
use swofty_core::install::Game;
use swofty_core::platform::process::game_processes;
use swofty_core::store::Store;
use swofty_i18n::{fill, tr, trf, trn};

use super::*;

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

/// The Restore game folder button: what `swofty restore` does.
pub fn restore(store: &Store, dir: PathBuf) -> Result<String, String> {
    let _helpers = store.helpers().pause();
    let g = Game::locate(&store.root, dir).map_err(|e| e.to_string())?;
    let done = recovery::restore(&store.root, &g).map_err(|e| e.to_string())?;
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
