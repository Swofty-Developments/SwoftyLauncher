//! Loader files missing from the game folder or from the launcher's own install.

use std::path::Path;

use crate::Result;
use crate::addons::builtin::{CORE_DLL, LOADER_ENV};
use crate::game_folder::baseline::sha256_file;
use crate::game_folder::placed::Placed;
use crate::install::PROXY;

use super::launcher_dir;

/// The DLLs the launcher wrote into the game folder, with the bytes it wrote.
pub fn placed_dlls(data: &Path) -> Result<Vec<(String, String)>> {
    Ok(Placed::load(data)?
        .files
        .into_values()
        .filter(|f| !f.replaced && f.path.to_ascii_lowercase().ends_with(".dll"))
        .map(|f| (f.path, f.sha256))
        .collect())
}

/// The placed DLLs that are gone from the game folder, cannot be read, or no
/// longer hold the bytes written there.
pub fn missing(game: &Path, dlls: &[(String, String)]) -> Vec<String> {
    dlls.iter()
        .filter(|(rel, sha)| !sha256_file(&game.join(rel)).is_ok_and(|now| now == *sha))
        .map(|(rel, _)| rel.clone())
        .collect()
}

/// The report's file list for an install with no loader folder at all.
pub const LOADER_FOLDER: &str = "loader/";

/// A loader folder of the install that lost version.dll or the core DLL, or
/// an installed launcher (in a `bin` folder) with no loader folder at all:
/// the build then finds no loader and the game starts without one.
pub fn damaged_install() -> Vec<String> {
    if crate::addons::builtin::loader_dir().is_some() || std::env::var_os(LOADER_ENV).is_some() {
        return Vec::new();
    }
    let Some(bin) = launcher_dir() else {
        return Vec::new();
    };
    damaged_in(&bin)
}

pub(super) fn damaged_in(bin: &Path) -> Vec<String> {
    let dirs = [bin.parent().map(|p| p.join("loader")), Some(bin.join("loader"))];
    match dirs.into_iter().flatten().find(|d| d.is_dir()) {
        Some(dir) => damaged_loader(&dir),
        None if bin.file_name().is_some_and(|n| n == "bin") => vec![LOADER_FOLDER.to_owned()],
        None => Vec::new(),
    }
}

pub(super) fn damaged_loader(dir: &Path) -> Vec<String> {
    [PROXY, CORE_DLL]
        .into_iter()
        .filter(|rel| !dir.join(rel).is_file())
        .map(|rel| format!("loader/{rel}"))
        .collect()
}
