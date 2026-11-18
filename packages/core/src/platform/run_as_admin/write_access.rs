//! Whether this process can create files in the game folder, a write Windows moves into the VirtualStore
//! counting as one it can't.

use std::path::{Path, PathBuf};

use super::elevation::elevated;

/// The folder under the game folder a modded session writes in first.
pub const FIRST_WRITE: &str = "2025_asset_groups/ui_package";

/// The first of the game folder and [`FIRST_WRITE`] in it that this process
/// can't create files in, such as a folder under Program Files that Windows
/// lets only administrators change.
pub fn unwritable_dir(game: &Path) -> Option<PathBuf> {
    [game.to_path_buf(), game.join(FIRST_WRITE)]
        .into_iter()
        .find(|dir| dir.is_dir() && !can_create_in(dir))
}

pub(super) fn can_create_in(dir: &Path) -> bool {
    can_create_in_with(dir, virtual_store)
}

/// A file Windows virtualised into the VirtualStore is one the game never
/// sees, so it counts as one this process can't create.
pub(super) fn can_create_in_with(dir: &Path, store: impl Fn(&Path) -> Option<PathBuf>) -> bool {
    let probe = dir.join(format!(".swofty-write-check-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let virtualised = store(&probe).is_some_and(|v| v.is_file());
            std::fs::remove_file(&probe).ok();
            !virtualised
        }
        Err(e) => !matches!(
            e.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
        ),
    }
}

/// Where Windows keeps this user's copy of `path` when it virtualises a write
/// to a folder only administrators can change, such as Program Files:
/// `%LOCALAPPDATA%\VirtualStore` with the path's drive left out.
pub fn virtual_store(path: &Path) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    virtual_store_in(Path::new(&std::env::var_os("LOCALAPPDATA")?), path)
}

pub(super) fn virtual_store_in(local: &Path, path: &Path) -> Option<PathBuf> {
    let p = path.to_string_lossy();
    let b = p.as_bytes();
    (b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/'))
        .then(|| local.join("VirtualStore").join(&p[3..]))
}

/// The folder a modded session in `game` can't write in, when this process
/// isn't elevated; None when it can, or when it runs as administrator.
pub fn write_blocker(game: &Path) -> Option<PathBuf> {
    unwritable_dir(game).filter(|_| !elevated())
}

/// Why a modded session can't put its files in `game`, and what to do.
pub fn write_blocked_text(game: &Path) -> String {
    write_blocked_text_for(game, cfg!(windows))
}

/// [`write_blocked_text`] as Windows, or another system, puts it.
pub fn write_blocked_text_for(game: &Path, windows: bool) -> String {
    if windows {
        format!(
            "F1 25 is installed in {}, a folder Windows lets only administrators change, so Swofty \
             can't add your mods there. Run Swofty Launcher as administrator, or move F1 25 to a \
             folder outside Program Files{}",
            game.display(),
            crate::game_folder::copies::move_hint(crate::install::Source::detect(game))
        )
    } else {
        format!(
            "Swofty can't write in the F1 25 folder {}, so it can't add your mods there. Give your \
             user write access to it",
            game.display()
        )
    }
}
