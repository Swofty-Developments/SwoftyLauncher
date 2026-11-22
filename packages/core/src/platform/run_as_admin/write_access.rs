//! Whether this process can create files in the game folder, a write Windows moves into the VirtualStore
//! counting as one it can't.

use std::path::{Path, PathBuf};

use super::elevation::elevated;
#[cfg(windows)]
use super::tools::windows_tool_within;

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

/// The folders under the game folder the mod loader builds archives in at
/// boot for the game's own file system to read, which reaches only files
/// under the game folder: the team logo and the garage art caches.
pub const CORE_CACHES: &[&str] = &["swofty/addons/swofty.logos", "swofty/addons/swofty.garage"];

/// The Users group, by SID so the grant reads the same in every Windows
/// language, gets Modify on a folder and everything made in it.
#[cfg(windows)]
const USERS_MODIFY: &str = "*S-1-5-32-545:(OI)(CI)M";

/// Makes each of [`CORE_CACHES`] in `game` one F1 25 can create files in,
/// when F1 25 run without administrator rights can't and this process runs
/// as administrator. F1 25 started by the EA app runs without administrator
/// rights even then, and a folder the administrator copy makes in a game
/// folder only administrators can change, such as one under Program Files on
/// any drive, lets it only read. Returns a line for each folder it opens and
/// each that stays closed to the game.
pub fn open_core_caches(game: &Path) -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    open_core_caches_with(
        game,
        super::unelevated::can_create_in,
        elevated,
        grant_users_modify,
    )
}

/// The folder `rel`, one of [`CORE_CACHES`], in `game`, with the separator
/// of this system between its parts.
pub fn core_cache_dir(game: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(game.to_path_buf(), |dir, part| dir.join(part))
}

/// [`open_core_caches`] with the calls that tell whether F1 25 run without
/// administrator rights can create files in a folder, whether this process
/// runs as administrator, and that open one folder to the game. A cache
/// folder that is not there yet counts as open when the game can create
/// files in the nearest folder above it, and one whose access can't be read
/// counts as closed.
pub(crate) fn open_core_caches_with(
    game: &Path,
    open: impl Fn(&Path) -> std::result::Result<bool, String>,
    elevated: impl FnOnce() -> bool,
    grant: impl Fn(&Path) -> std::result::Result<(), String>,
) -> Vec<String> {
    let closed: Vec<&str> = CORE_CACHES
        .iter()
        .copied()
        .filter(|rel| {
            let dir = core_cache_dir(game, rel);
            let nearest = dir
                .ancestors()
                .take_while(|d| d.starts_with(game))
                .find(|d| d.is_dir())
                .unwrap_or(game);
            open(nearest) != Ok(true)
        })
        .collect();
    if closed.is_empty() || !elevated() {
        return Vec::new();
    }
    closed
        .into_iter()
        .map(|rel| {
            let dir = core_cache_dir(game, rel);
            match std::fs::create_dir_all(&dir)
                .map_err(|e| e.to_string())
                .and_then(|()| grant(&dir))
            {
                Ok(()) => format!(
                    "core caches: gave the Users group Modify on {rel}, for F1 25 run without administrator rights"
                ),
                Err(why) => format!(
                    "core caches: F1 25 run without administrator rights can't create files in {rel}, \
                     and the archives the mod loader builds there at boot stay out of the game: {why}"
                ),
            }
        })
        .collect()
}

#[cfg(windows)]
pub(super) fn grant_users_modify(dir: &Path) -> std::result::Result<(), String> {
    let path = dir
        .to_str()
        .ok_or_else(|| format!("{} is not a path icacls can be given", dir.display()))?;
    windows_tool_within(
        "icacls",
        &[path, "/grant", USERS_MODIFY, "/Q"],
        std::time::Duration::from_secs(30),
    )
    .map(drop)
}

#[cfg(not(windows))]
fn grant_users_modify(_: &Path) -> std::result::Result<(), String> {
    Ok(())
}
