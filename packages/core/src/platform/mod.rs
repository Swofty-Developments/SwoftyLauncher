//! The computer around the game: Steam and the EA app, running processes, drives, and the Windows
//! settings and policies that decide whether a program may start.

pub mod app_control;
pub mod disk;
pub mod ea_app;
pub mod process;
pub mod run_as_admin;
pub mod steam;

use std::path::{Path, PathBuf};

use crate::install::Source;

pub use ea_app::EaApp;
pub use steam::Steam;

/// A store that installs F1 25 and starts it for the player.
pub trait LaunchPlatform: Sync {
    /// The store, as the launcher names it to the player.
    fn name(&self) -> &'static str;

    fn source(&self) -> Source;

    /// Whether this store installed the copy of the game in `dir`.
    fn installed(&self, dir: &Path) -> bool;

    /// The folders this store has F1 25 installed in on this computer.
    fn installs(&self) -> Vec<PathBuf>;

    /// Whether the store's own program is running.
    fn client_running(&self) -> bool;

    /// How the player moves the copy to another folder, after "move it".
    fn move_hint(&self) -> &'static str;
}

/// Every store, in the order a folder is checked against them.
pub const PLATFORMS: [&dyn LaunchPlatform; 2] = [&Steam, &EaApp];

/// The store `source` names, or `None` for a folder no store installed.
pub fn for_source(source: Source) -> Option<&'static dyn LaunchPlatform> {
    PLATFORMS.into_iter().find(|p| p.source() == source)
}

/// The store that installed the copy of the game in `dir`.
pub fn installed_by(dir: &Path) -> Option<&'static dyn LaunchPlatform> {
    PLATFORMS.into_iter().find(|p| p.installed(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-platform-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn a_folder_is_installed_by_the_store_its_source_names() {
        let root = tmp("detect");
        let steam = root.join("steamapps/common/F1 25");
        std::fs::create_dir_all(&steam).unwrap();
        std::fs::write(
            root.join(format!("steamapps/appmanifest_{}.acf", crate::STEAM_APP_ID)),
            "",
        )
        .unwrap();
        let javelin = root.join("javelin");
        std::fs::create_dir_all(&javelin).unwrap();
        std::fs::write(javelin.join("EAJavelinInstaller_installscript.vdf"), "").unwrap();
        let ea = root.join("ea");
        std::fs::create_dir_all(ea.join("__Installer")).unwrap();
        std::fs::write(ea.join(ea_app::MARKER), "<contentID>1</contentID>").unwrap();
        let other = root.join("other");
        std::fs::create_dir_all(&other).unwrap();
        for dir in [&steam, &javelin, &ea, &other] {
            let by = installed_by(dir).map_or(Source::Unknown, |p| p.source());
            assert_eq!(by, Source::detect(dir), "{}", dir.display());
        }
        assert_eq!(installed_by(&ea).map(|p| p.name()), Some("EA app"));
        assert_eq!(for_source(Source::Steam).map(|p| p.name()), Some("Steam"));
        assert!(for_source(Source::Unknown).is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
