use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::store::Store;
use crate::{Error, GAME_EXE, Result};

mod addons;
pub mod automatic;
mod collect;
mod crashes;
mod files;
pub mod perf;
mod redact;
mod summary;
mod system;
pub mod upload;

pub use automatic::{Automatic, Outcome, RoomRun};
pub use collect::newest_sessions;
pub use crashes::{Crash, newest_crash, recent_crashes, summarise_crash};
use files::{file_name, modified};
pub use redact::{redact_env, redact_json, redact_page_tokens, secret_name};
pub use system::os_description;
pub use upload::{KIND_HEADER, LogServer, SendError, Sent, Stage};

pub const UNAVAILABLE: &str = "Sending logs isn't available in this build";
pub const NAME_MIN: usize = 2;
pub const NAME_MAX: usize = 32;
pub const MESSAGE_MIN: usize = 10;
pub const MESSAGE_MAX: usize = 1000;
pub const FILE_CAP: u64 = 5 << 20;
pub const TOTAL_CAP: u64 = 40 << 20;
pub const PART_LIMIT: usize = 9 << 20;
pub const SESSIONS: usize = 3;
pub const SNAPSHOT: &str = "swofty/addons/swofty.career-editor/snapshot.json";
const CAREER_EDITOR: &str = "swofty.career-editor";
pub const PERF_LINES: usize = 100;
const LOADER_HEAD: u64 = 16 << 10;
pub const LOADER_DIR: &str = "swofty";
pub const KEPT_DIR: &str = "logs";
pub const KEPT: usize = 10;
pub const LAUNCH_LOGS: [&str; 2] = ["state/launcher-run.log", "state/wrapper-run.log"];
pub const NET_TRACE: &str = "swofty/net-trace.log";
pub const NET_TRACE_CAP: u64 = 16 << 20;
/// How many saved fingerprint comparisons a bundle carries, newest first.
const PRINTS_SENT: usize = 3;

const REDACTED: &str = "[redacted]";
const SESSION_DEPTH: usize = 3;
const MIN_TAIL: u64 = 64 << 10;
/// A cut loader.log keeps this much of its start, the boot header: versions, loaded modules,
/// hardware settings, bindings and the addon list. The rest of the file's cap goes to its end.
const LOADER_KEEP: u64 = 2560 << 10;
/// A cut loader.log whose window stage falls in the part the cut leaves out keeps this much from the
/// stage's first line: the core's window-stage hooks and every addon's hook installs.
const WINDOW_KEEP: u64 = 512 << 10;
const GAP_NOTE: u64 = 128;
const PERF_READ: u64 = 1 << 20;
const DUMP_CAP: u64 = 24 << 20;
/// How many crash reports a bundle carries, newest first; only the newest keeps its minidump, the others
/// are listed in `crash/dumps.txt`.
pub const CRASHES_SENT: usize = 5;
const OVERLAY_READ: u64 = 32 << 20;

/// Where the logs go: `SWOFTY_LOGS_URL` at run time, else the address baked in
/// at build time, if any.
pub fn destination() -> Option<LogServer> {
    let runtime = std::env::var("SWOFTY_LOGS_URL").ok();
    LogServer::new(runtime.as_deref().or(option_env!("SWOFTY_LOGS_URL")))
}

pub fn available() -> bool {
    destination().is_some()
}

pub fn send(bundle: &Bundle, progress: &dyn Fn(Stage)) -> std::result::Result<Sent, SendError> {
    destination()
        .ok_or(SendError::Unavailable)?
        .send(bundle, progress)
}

/// `$DATA/logs`, where the launcher and `swofty logs send` keep the newest
/// bundles for the player to share themselves.
pub fn kept_dir(store: &Store) -> PathBuf {
    store.root.join(KEPT_DIR)
}

/// Removes the bundles in `dir` beyond the newest `keep`, counting `newest`
/// and never removing it. Returns the removed files.
pub fn prune(dir: &Path, keep: usize, newest: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut older: Vec<_> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p != newest && is_bundle(p))
        .map(|p| (modified(&p), p))
        .collect();
    older.sort_by(|a, b| b.cmp(a));
    older
        .into_iter()
        .skip(keep.saturating_sub(1))
        .filter_map(|(_, p)| std::fs::remove_file(&p).ok().map(|()| p))
        .collect()
}

fn is_bundle(path: &Path) -> bool {
    let name = file_name(path);
    name.starts_with("swofty-logs-") && name.ends_with(".zip") && path.is_file()
}

pub fn check_name(raw: &str) -> std::result::Result<String, String> {
    let name = raw.trim();
    match name.chars().count() {
        0 => Err("Enter your Discord username".into()),
        n if n < NAME_MIN => Err(format!("A Discord username has at least {NAME_MIN} characters")),
        n if n > NAME_MAX => Err(format!("A Discord username has at most {NAME_MAX} characters")),
        _ => Ok(name.to_owned()),
    }
}

pub fn check_message(raw: &str) -> std::result::Result<String, String> {
    let message = raw.trim();
    match message.chars().count() {
        n if n < MESSAGE_MIN => Err(format!(
            "Describe what went wrong in at least {MESSAGE_MIN} characters"
        )),
        n if n > MESSAGE_MAX => Err(format!("Keep it to {MESSAGE_MAX} characters")),
        _ => Ok(message.to_owned()),
    }
}

/// Whether the data folder holds a launch log or a game session, the logs
/// that show what happened when the game ran.
pub fn has_session(store: &Store) -> bool {
    LAUNCH_LOGS.iter().any(|l| store.root.join(l).is_file()) || !newest_sessions(store, 1).is_empty()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameInfo {
    pub dir: Option<PathBuf>,
    pub build_id: Option<u64>,
    pub version: Option<String>,
}

impl GameInfo {
    /// The copy the launcher plays: the one picked in its game switcher,
    /// else Steam's.
    pub fn detect(store: &Store) -> Self {
        let Some(copy) = crate::game_folder::copies::find_active(&store.config()) else {
            return Self::default();
        };
        let (dir, build_id) = (copy.dir.clone(), copy.build_id());
        Self {
            version: crate::game_folder::exe_version::read(&dir.join(GAME_EXE)),
            dir: Some(dir),
            build_id,
        }
    }

    pub fn describe(&self) -> String {
        if self.dir.is_none() {
            return "not found".into();
        }
        match (&self.version, self.build_id) {
            (Some(v), Some(b)) => format!("{v} (Steam build {b})"),
            (Some(v), None) => v.clone(),
            (None, Some(b)) => format!("Steam build {b}"),
            (None, None) => "found, version unknown".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub name: String,
    pub message: String,
    pub game: GameInfo,
    /// The running room's state, already redacted, sent as `multiplayer/session.json`.
    pub room: Option<Value>,
    /// Set when the launcher sends the bundle by itself, without asking the player.
    pub automatic: Option<Automatic>,
}

impl Report {
    /// The report the launcher sends by itself for `automatic`, with its message in place of the player's.
    pub fn automatic(name: String, game: GameInfo, room: Option<Value>, automatic: Automatic) -> Self {
        Self {
            name,
            message: automatic.message(),
            game,
            room,
            automatic: Some(automatic),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: String,
    pub data: Vec<u8>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Bundle {
    pub stem: String,
    pub name: String,
    pub message: String,
    pub launcher: String,
    pub os: String,
    pub game: String,
    pub entries: Vec<Entry>,
    pub skipped: Vec<String>,
    /// The launcher sent it by itself after a crash, which every part says to the log server.
    pub automatic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub file_name: String,
    pub data: Vec<u8>,
    pub index: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub file: u64,
    pub total: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            file: FILE_CAP,
            total: TOTAL_CAP,
        }
    }
}

impl Bundle {
    pub fn listing(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| match &e.note {
                Some(n) => format!("{}  {}  ({n})", e.path, size(e.data.len() as u64)),
                None => format!("{}  {}", e.path, size(e.data.len() as u64)),
            })
            .collect()
    }

    pub fn file_name(&self) -> String {
        format!("{}.zip", self.stem)
    }

    pub fn zip(&self) -> Result<Vec<u8>> {
        zip_entries(self.entries.iter())
    }

    /// Writes the whole zip into `dir` as [`Bundle::file_name`], then prunes
    /// `dir` to the newest `keep` bundles.
    pub fn save(&self, dir: &Path, keep: usize) -> Result<PathBuf> {
        let data = self.zip()?;
        std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
        let path = dir.join(self.file_name());
        let partial = dir.join(format!("{}.tmp", self.file_name()));
        std::fs::write(&partial, &data).map_err(crate::io_err(&partial))?;
        std::fs::rename(&partial, &path).map_err(crate::io_err(&path))?;
        prune(dir, keep, &path);
        Ok(path)
    }

    /// The zip files to upload, each at most `limit` bytes where possible:
    /// one file when everything fits, else the entries packed in order into
    /// standalone zips named `-part<n>`.
    pub fn parts(&self, limit: usize) -> Result<Vec<Part>> {
        let whole = self.zip()?;
        if whole.len() <= limit {
            return Ok(vec![Part {
                file_name: self.file_name(),
                data: whole,
                index: 1,
                total: 1,
            }]);
        }
        let mut groups: Vec<Vec<&Entry>> = vec![Vec::new()];
        let mut used = 0usize;
        for e in &self.entries {
            let cost = compressed_len(&e.data) + 2 * e.path.len() + 128;
            let group = groups.last_mut().expect("never empty");
            if !group.is_empty() && used + cost > limit {
                groups.push(Vec::new());
                used = 0;
            }
            groups.last_mut().expect("never empty").push(e);
            used += cost;
        }
        let total = groups.len();
        groups
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                Ok(Part {
                    file_name: format!("{}-part{}.zip", self.stem, i + 1),
                    data: zip_entries(g.into_iter())?,
                    index: i + 1,
                    total,
                })
            })
            .collect()
    }
}

fn zip_entries<'a>(entries: impl Iterator<Item = &'a Entry>) -> Result<Vec<u8>> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(false);
    for e in entries {
        z.start_file(e.path.as_str(), opts)?;
        z.write_all(&e.data)
            .map_err(|e| Error::Other(format!("writing the logs zip: {e}")))?;
    }
    Ok(z.finish()?.into_inner())
}

fn compressed_len(data: &[u8]) -> usize {
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).ok();
    enc.finish().map_or(data.len(), |v| v.len())
}

fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches(['.', '_']).to_owned();
    if s.is_empty() { "player".into() } else { s }
}

pub fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1 << 20 => format!("{:.1} KB", b as f64 / 1024.),
        b => format!("{:.1} MB", b as f64 / f64::from(1 << 20)),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `YYYYMMDD-HHMMSS` in UTC.
pub fn utc_stamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn readable(stamp: &str) -> String {
    match (
        stamp.get(..4),
        stamp.get(4..6),
        stamp.get(6..8),
        stamp.get(9..11),
        stamp.get(11..13),
        stamp.get(13..15),
    ) {
        (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) if stamp.len() == 15 => {
            format!("{y}-{mo}-{d} {h}:{mi}:{s} UTC")
        }
        _ => stamp.to_owned(),
    }
}

#[cfg(test)]
mod tests;
