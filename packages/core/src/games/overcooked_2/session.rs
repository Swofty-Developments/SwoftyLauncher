//! The session file the launcher writes at Play and the core reads: the game folder, the data folder, where the
//! core logs and reports, and the addons on in load order.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::addons::Store;
use crate::{Result, io_err};

pub const ENV: &str = "SWOFTY_OC2_SESSION";
pub const SESSION_FILE: &str = "session.json";
pub const LOG_FILE: &str = "core.log";
pub const STATUS_FILE: &str = "status.json";
pub const GAME_LOG_FILE: &str = "player.log";
pub const OUTPUT_FILE: &str = "output.log";
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAddon {
    pub id: String,
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    pub assemblies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub schema: u32,
    pub launcher: String,
    pub game_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log: PathBuf,
    pub status: PathBuf,
    pub addons: Vec<SessionAddon>,
}

/// What the core writes to `status` once the addons are loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub core: Option<String>,
    #[serde(default)]
    pub std: Option<String>,
    #[serde(default)]
    pub loaded: Vec<String>,
    #[serde(default)]
    pub failed: Vec<Failed>,
    #[serde(default)]
    pub main_menu: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Failed {
    pub id: String,
    #[serde(default)]
    pub error: String,
}

/// A written session: its folder and `session.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub dir: PathBuf,
    pub path: PathBuf,
    pub session: Session,
}

impl Written {
    pub fn status(&self) -> Option<Status> {
        read_status(&self.dir)
    }
}

pub fn read_status(dir: &Path) -> Option<Status> {
    crate::read_json(&dir.join(STATUS_FILE)).ok()
}

/// `20261129-001502` for the UTC time `secs`.
pub fn stamp(secs: i64) -> String {
    let iso = crate::catalog::iso(secs);
    iso.chars()
        .filter(|c| c.is_ascii_digit() || *c == 'T')
        .collect::<String>()
        .replace('T', "-")
}

/// The next unused session folder for the time `secs`.
pub fn next_dir(store: &Store, secs: i64) -> PathBuf {
    let sessions = store.sessions_dir();
    let base = stamp(secs);
    let dir = (1..)
        .map(|n| match n {
            1 => sessions.join(&base),
            n => sessions.join(format!("{base}-{n}")),
        })
        .find(|d| !d.exists())
        .expect("an unused session folder");
    std::path::absolute(&dir).unwrap_or(dir)
}

/// The session for the copy in `game_dir` with the addons on in `store`, kept in `dir`.
pub fn build(store: &Store, game_dir: &Path, launcher: &str, dir: &Path) -> Session {
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    Session {
        schema: SCHEMA,
        launcher: launcher.to_owned(),
        game_dir: abs(game_dir),
        data_dir: abs(&store.dir),
        log: dir.join(LOG_FILE),
        status: dir.join(STATUS_FILE),
        addons: store
            .on()
            .into_iter()
            .map(|a| SessionAddon {
                id: a.manifest.id,
                name: a.manifest.name,
                version: a.manifest.version.to_string(),
                dir: abs(&a.dir),
                assemblies: a.manifest.assemblies,
            })
            .collect(),
    }
}

/// Writes a new session for the copy in `game_dir` with the addons on in `store`, under its sessions folder.
pub fn write(store: &Store, game_dir: &Path, launcher: &str, secs: i64) -> Result<Written> {
    let dir = next_dir(store, secs);
    std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let session = build(store, game_dir, launcher, &dir);
    let path = dir.join(SESSION_FILE);
    crate::write_json(&path, &session)?;
    Ok(Written { dir, path, session })
}

/// The newest session folder, when there is one.
pub fn latest(store: &Store) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(store.sessions_dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(SESSION_FILE).is_file())
        .collect();
    dirs.sort();
    dirs.pop()
}
