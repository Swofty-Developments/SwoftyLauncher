//! Modded sessions and what an unfinished one leaves behind.
//!
//! A `swofty run` session holds [`SessionLock`] from before it installs the
//! profile until its game files are rolled back, so nothing else rolls back,
//! restores or installs underneath it. The operating system drops the lock
//! when the session's process ends for any reason, which is how a later start
//! tells an unfinished session (its journal still open, the lock free) from a
//! live one, and [`recover`] puts the game folder back.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::baseline::{Baseline, DepotHashes, FileRecord, Objects};
use crate::install::{Conflict, Drift, Game, Installer, RolledBack};
use crate::{Error, Result, io_err, read_json, write_json};

pub const HEADLINE: &str = "Recovered the game folder from an unfinished session";

pub fn lock_path(data: &Path) -> PathBuf {
    data.join("state/session.lock")
}

#[derive(Debug)]
pub struct SessionLock {
    _file: File,
}

impl SessionLock {
    /// Take the lock, or `None` while another process holds it. Where the
    /// file system cannot lock files at all, sessions go unguarded rather
    /// than refused.
    pub fn try_acquire(data: &Path) -> Result<Option<Self>> {
        let path = lock_path(data);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(io_err(&path))?;
        match file.try_lock() {
            Err(TryLockError::WouldBlock) => Ok(None),
            Ok(()) | Err(TryLockError::Error(_)) => Ok(Some(Self { _file: file })),
        }
    }

    /// Take the lock once its holder lets go, giving up after `timeout`.
    pub fn wait(data: &Path, timeout: Duration) -> Result<Option<Self>> {
        let start = Instant::now();
        loop {
            if let Some(lock) = Self::try_acquire(data)? {
                return Ok(Some(lock));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    pub fn held_elsewhere(data: &Path) -> bool {
        matches!(Self::try_acquire(data), Ok(None))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Only what `swofty run` sessions opened; a `swofty apply` stays.
    Sessions,
    /// Every open journal, as a new session or install starts from vanilla.
    All,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recovered {
    pub offline: bool,
    /// What an unreadable offline play record made the roll back do.
    pub offline_notes: Vec<String>,
    pub relocated: Option<String>,
    pub journals: Vec<RolledBack>,
    pub swept: Vec<String>,
    pub cleared: Vec<String>,
    pub problems: Vec<String>,
    pub logs: usize,
    /// The build the last session was running when its process ended.
    pub interrupted: Option<crate::busy::Busy>,
}

impl Recovered {
    /// Whether an unfinished session left anything that was put back.
    pub fn recovered(&self) -> bool {
        self.offline
            || self.journals.iter().any(|j| j.session)
            || !self.swept.is_empty()
            || !self.cleared.is_empty()
    }

    /// Whether everything an unfinished session left is back. A file that
    /// stayed locked keeps the record that owns it, so the next start tries
    /// again.
    pub fn complete(&self) -> bool {
        self.problems.is_empty()
    }

    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.extend(self.relocated.iter().cloned());
        if self.offline {
            out.push("rolled back the offline play staging".to_owned());
        }
        out.extend(self.offline_notes.iter().cloned());
        for j in &self.journals {
            let what = if j.session {
                "a session left open"
            } else {
                "swofty apply installed"
            };
            out.push(format!(
                "rolled back journal {} ({} change(s) for profile {}) that {what}",
                j.id, j.changes, j.profile
            ));
            out.extend(j.drift.iter().map(ToString::to_string));
            if !j.kept.is_empty() {
                out.push(format!(
                    "kept {} file(s) addons wrote, for the next session with them",
                    j.kept.len()
                ));
            }
        }
        out.extend(self.swept.iter().cloned());
        out.extend(self.cleared.iter().cloned());
        out.extend(self.problems.iter().cloned());
        if let Some(b) = &self.interrupted {
            out.push(format!(
                "the last Play stopped while {}; Play starts the build again",
                b.progress()
            ));
        }
        out
    }
}

/// Put back what unfinished sessions left in the game folder: the offline
/// play staging, open journals in `scope`, files the placed-files record
/// proves are the launcher's, and the loader folder's leftover logs (moved to
/// `sessions/orphaned-logs`). Holding `lock` proves no session is live.
pub fn recover(lock: &SessionLock, data: &Path, game: &Game, scope: Scope) -> Result<Recovered> {
    let _ = lock;
    let mut r = Recovered::default();
    match relocate_offline(data, &game.dir) {
        Ok(note) => {
            r.relocated = note;
            match crate::offline::restore_pending(data, &game.dir) {
                Ok(done) => {
                    r.offline = done.is_some();
                    r.offline_notes = done.unwrap_or_default();
                }
                Err(e) => r.problems.push(format!("offline play staging: {e}")),
            }
        }
        Err(e) => r.problems.push(format!("offline play staging: {e}")),
    }
    let inst = Installer::new(data, game);
    r.journals = inst.roll_back(scope == Scope::Sessions)?;
    r.swept = inst.sweep()?;
    r.logs = inst.tidy(&data.join("sessions/orphaned-logs"))?;
    r.interrupted = crate::busy::Busy::read(data);
    crate::busy::Busy::clear(data);
    crate::busy::clear_cancel(data);
    if r.recovered() {
        Notice::record(data, &r.lines())?;
    }
    Ok(r)
}

/// The last recovery, for the launcher to announce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub at: f64,
    pub lines: Vec<String>,
}

impl Notice {
    pub fn path(data: &Path) -> PathBuf {
        data.join("state/recovered.json")
    }

    pub fn record(data: &Path, lines: &[String]) -> Result<()> {
        write_json(
            &Self::path(data),
            &Self {
                at: now_secs(),
                lines: lines.to_vec(),
            },
        )
    }

    pub fn read(data: &Path) -> Option<Self> {
        read_json(&Self::path(data)).ok()
    }
}

/// Files of unknown provenance that stopped the last launch. They are only
/// ever moved aside, by an explicit restore, never deleted or overwritten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blocked {
    pub game: PathBuf,
    pub files: Vec<BlockedFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockedFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replaced: bool,
}

impl Blocked {
    pub fn path(data: &Path) -> PathBuf {
        data.join("state/blocked.json")
    }

    pub fn record(data: &Path, game: &Path, conflicts: &[Conflict]) -> Result<Self> {
        let b = Self {
            game: game.to_path_buf(),
            files: conflicts
                .iter()
                .filter(|c| !c.owned)
                .map(|c| BlockedFile {
                    path: c.path.clone(),
                    sha256: c.sha256.clone(),
                    replaced: c.replaced,
                })
                .collect(),
        };
        write_json(&Self::path(data), &b)?;
        Ok(b)
    }

    pub fn read(data: &Path) -> Option<Self> {
        read_json(&Self::path(data)).ok()
    }

    pub fn clear(data: &Path) -> Result<()> {
        let path = Self::path(data);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io_err(&path)(e)),
            _ => Ok(()),
        }
    }

    /// Whether every file is one added under the game folder's `swofty`
    /// folder, which only the launcher writes to: what a session that was
    /// never rolled back left there, with the record of it lost.
    pub fn ours(&self) -> bool {
        !self.files.is_empty()
            && self
                .files
                .iter()
                .all(|f| !f.replaced && f.path.starts_with("swofty/"))
    }

    /// What a restore does about these files, for the error a player sees.
    pub fn explain(&self, data: &Path) -> String {
        let replaced = self.files.iter().any(|f| f.replaced);
        format!(
            "Restore game folder (swofty restore) rolls back swofty's own changes and moves {} to {}, where you can get {} back{}",
            if self.files.len() == 1 {
                "this file"
            } else {
                "these files"
            },
            set_aside_dir(data).display(),
            if self.files.len() == 1 { "it" } else { "them" },
            if replaced {
                "; the game's original files go back in their place"
            } else {
                ""
            }
        )
    }
}

pub fn set_aside_dir(data: &Path) -> PathBuf {
    data.join("set-aside")
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Restored {
    pub notes: Vec<String>,
    pub set_aside: Vec<PathBuf>,
    pub left: Vec<Drift>,
}

/// `swofty restore`: roll back every journal and the offline play staging,
/// move the files that blocked the last launch aside, and put the game folder
/// back to the baseline. A session that holds the lock refuses it at once.
pub fn restore(data: &Path, game: &Game) -> Result<Restored> {
    let lock = wait_for_lock(data, Wait::NONE, &|| crate::process::game_running_in(&game.dir))?;
    let out = restore_locked(&lock, data, game)?;
    drop(lock);
    Ok(out)
}

pub const SESSION_HELD: &str = "the last Play is still starting F1 25 or putting the game files back, with F1 25 closed; \
     close any Steam or EA app window that is starting F1 25, wait a minute, then restore the game folder again";

pub const GAME_RUNNING: &str = "F1 25 is still running; close F1 25, then restore the game folder again";

/// How long a restore waits for the session lock. A Play still building or
/// checking the game files is waited for up to `build`; a session that holds
/// the lock with F1 25 closed (starting the game, or putting its files back)
/// up to `other`. A running F1 25 is never waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wait {
    pub build: Duration,
    pub other: Duration,
}

impl Wait {
    pub const NONE: Self = Self {
        build: Duration::ZERO,
        other: Duration::ZERO,
    };
}

const LOCK_POLL: Duration = Duration::from_millis(250);

/// Take the session lock for a restore, or say what holds it: a Play's build
/// (by its progress), a running F1 25, or a session that is starting the game
/// or putting its files back.
pub fn wait_for_lock(data: &Path, wait: Wait, game_running: &dyn Fn() -> bool) -> Result<SessionLock> {
    let start = Instant::now();
    let mut other_since: Option<Instant> = None;
    loop {
        if let Some(lock) = SessionLock::try_acquire(data)? {
            return Ok(lock);
        }
        let build = crate::busy::Busy::live(data);
        if build.is_none() && game_running() {
            return Err(Error::Other(GAME_RUNNING.into()));
        }
        match build {
            Some(b) => {
                other_since = None;
                if start.elapsed() >= wait.build {
                    return Err(Error::Other(b.restore_text()));
                }
            }
            None => {
                if other_since.get_or_insert_with(Instant::now).elapsed() >= wait.other {
                    return Err(Error::Other(SESSION_HELD.into()));
                }
            }
        }
        std::thread::sleep(LOCK_POLL);
    }
}

/// [`restore`] under a session lock the caller holds.
pub fn restore_locked(lock: &SessionLock, data: &Path, game: &Game) -> Result<Restored> {
    let depots = crate::steam::find_game()
        .filter(|i| same_dir(&i.dir, &game.dir))
        .and_then(|i| crate::stock::Depots::load(&i).ok());
    let steam: &dyn DepotHashes = match &depots {
        Some(d) => d,
        None => &(),
    };
    restore_locked_with(lock, data, game, steam)
}

/// [`restore_locked`] with Steam's list of the game's files. A file that
/// blocked the last launch and is Steam's copy, as after a Verify, stays in
/// place and becomes the original on record.
pub fn restore_locked_with(
    _lock: &SessionLock,
    data: &Path,
    game: &Game,
    steam: &dyn DepotHashes,
) -> Result<Restored> {
    let mut out = Restored::default();
    out.notes.extend(relocate_offline(data, &game.dir)?);
    if let Some(notes) = crate::offline::restore_pending(data, &game.dir)? {
        out.notes.push("offline play staging".into());
        out.notes.extend(notes);
    }
    if let Some(mut blocked) = Blocked::read(data) {
        if same_dir(&blocked.game, &game.dir) {
            let kept = keep_stock(data, game, &blocked, steam)?;
            if !kept.is_empty() {
                out.notes.push(format!(
                    "{} file(s) in the way are Steam's copy; they stay and are the originals on record now",
                    kept.len()
                ));
            }
            blocked.files.retain(|f| !kept.contains(&f.path));
            out.set_aside = set_aside(data, game, &blocked)?;
        }
        Blocked::clear(data)?;
    }
    let (left, proxy) = Installer::new(data, game).restore_setting_aside()?;
    out.left = left;
    out.set_aside.extend(proxy);
    Ok(out)
}

/// Point offline play's record at `game` when the folder it names no
/// longer has the game and `game` holds the files it placed, with the
/// digests it recorded. A record whose files are not there is refused.
pub fn relocate_offline(data: &Path, game: &Path) -> Result<Option<String>> {
    let journal = crate::offline::journal_path(data);
    let Some(mut record) = crate::offline::Journal::read(&journal)? else {
        return Ok(None);
    };
    let lines = &mut record.lines;
    let field = |l: &serde_json::Value, k: &str| l.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
    let Some(was) = lines
        .iter()
        .find(|l| field(l, "op") == "meta")
        .map(|l| PathBuf::from(field(l, "game_dir")))
        .filter(|d| !d.as_os_str().is_empty())
    else {
        return Ok(None);
    };
    if was.join(crate::GAME_EXE).is_file() || same_dir(&was, game) {
        return Ok(None);
    }
    let moved = |p: &str| Path::new(p).strip_prefix(&was).ok().map(|rel| game.join(rel));
    let slots: Vec<String> = lines
        .iter()
        .filter(|l| field(l, "op") == "rename")
        .map(|l| field(l, "from"))
        .collect();
    let (mut found, mut changed) = (0, Vec::new());
    for l in lines.iter() {
        let (op, to, want) = (field(l, "op"), field(l, "to"), field(l, "sha256"));
        let Some(here) = moved(&to) else {
            continue;
        };
        let Some(have) = crate::baseline::sha256_opt(&here)? else {
            continue;
        };
        match op.as_str() {
            "copy" if have == want => found += 1,
            "copy" if slots.contains(&to) => {}
            "rename" if want.is_empty() || have == want => found += 1,
            "copy" | "rename" => changed.push(here.display().to_string()),
            _ => {}
        }
    }
    if found == 0 || !changed.is_empty() {
        return Err(Error::Other(format!(
            "offline play's record is for F1 25 in {}, which is gone, and {} doesn't hold the files it placed{}, so \
             nothing was put back. Move the F1 25 folder back to {} and try again",
            was.display(),
            game.display(),
            if changed.is_empty() {
                String::new()
            } else {
                format!(" ({} changed)", changed.join(", "))
            },
            was.display()
        )));
    }
    for l in lines.iter_mut() {
        for k in ["from", "to"] {
            if let Some(here) = moved(&field(l, k)) {
                l[k] = here.display().to_string().into();
            }
        }
        if field(l, "op") == "meta" {
            l["game_dir"] = game.display().to_string().into();
        }
    }
    record.write(&journal)?;
    Ok(Some(format!(
        "offline play's record was for F1 25 in {}, which is gone; its files are in {}, so they are put back there",
        was.display(),
        game.display()
    )))
}

/// Back up each replaced file in `blocked` that is Steam's copy as its
/// original, and name them.
fn keep_stock(data: &Path, game: &Game, blocked: &Blocked, steam: &dyn DepotHashes) -> Result<Vec<String>> {
    let mut kept = Vec::new();
    let mut baseline = Baseline::load_or_new(data, &game.identity)?;
    let objects = Objects {
        root: data.join("baseline/objects"),
    };
    for f in blocked.files.iter().filter(|f| f.replaced) {
        let path = game.dir.join(&f.path);
        if !path.is_file() || steam.stock(&f.path, &path)? != Some(true) {
            continue;
        }
        let size = std::fs::metadata(&path).map_err(io_err(&path))?.len();
        let sha256 = objects.put_file(&path)?;
        baseline.files.insert(
            f.path.clone(),
            FileRecord {
                size,
                sha256,
                source: crate::baseline::Source::Depot,
            },
        );
        kept.push(f.path.clone());
    }
    if !kept.is_empty() {
        baseline.save(data)?;
    }
    Ok(kept)
}

fn set_aside(data: &Path, game: &Game, blocked: &Blocked) -> Result<Vec<PathBuf>> {
    let stamp = format!("{}", now_secs() as u64);
    let mut moved = Vec::new();
    for f in &blocked.files {
        let from = game.dir.join(&f.path);
        if !from.is_file() {
            continue;
        }
        let to = set_aside_dir(data).join(&stamp).join(&f.path);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        crate::move_file(&from, &to)?;
        moved.push(to);
        if f.replaced {
            continue;
        }
        for dir in from.ancestors().skip(1) {
            if dir == game.dir || !dir.starts_with(&game.dir) {
                break;
            }
            if std::fs::remove_dir(dir).is_err() {
                break;
            }
        }
    }
    Ok(moved)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (canon(a), canon(b));
    if cfg!(windows) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
