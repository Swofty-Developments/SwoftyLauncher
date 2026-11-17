//! The computer around the game: Steam and the EA app, running processes, drives, and the Windows
//! settings and policies that decide whether a program may start.

pub mod app_control;
pub mod disk;
pub mod ea_app;
pub mod process;
pub mod run_as_admin;
pub mod steam;
pub mod steam_client;
pub mod vdf;

use std::path::{Path, PathBuf};
#[cfg(any(windows, test))]
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

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

/// How long a record read with a Windows tool is kept. Steam and the EA app change their install records
/// only when they install, move or remove something, one Play reads them several times, and each read
/// starts a `reg query` process.
#[cfg(windows)]
pub(crate) const KEEP_RECORDS: Duration = Duration::from_secs(30);

/// Answers kept by key for a while, so lookups close together start a Windows tool once.
#[cfg(any(windows, test))]
pub(crate) struct Kept<K, V>(Mutex<BTreeMap<K, (Instant, V)>>);

#[cfg(any(windows, test))]
impl<K: Ord, V: Clone> Kept<K, V> {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(BTreeMap::new()))
    }

    /// The answer kept for `key` while it is younger than `keep`, else `find`'s, which is then kept.
    pub(crate) fn get(&self, key: K, keep: Duration, find: impl FnOnce() -> V) -> V {
        let mut kept = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, answer)) = kept.get(&key)
            && at.elapsed() < keep
        {
            return answer.clone();
        }
        let answer = find();
        kept.insert(key, (Instant::now(), answer.clone()));
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_answer_is_reused_per_key_until_it_is_too_old() {
        let kept = Kept::new();
        let runs = std::cell::Cell::new(0);
        let find = || {
            runs.set(runs.get() + 1);
            runs.get()
        };
        let minute = Duration::from_secs(60);
        assert_eq!(kept.get("a", minute, find), 1);
        assert_eq!(kept.get("a", minute, find), 1);
        assert_eq!(kept.get("b", minute, find), 2);
        assert_eq!(kept.get("a", Duration::ZERO, find), 3);
        assert_eq!(kept.get("a", minute, find), 3);
    }

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
