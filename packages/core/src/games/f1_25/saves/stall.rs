//! Profile saves that keep F1 25 on Loading... after Press any button.
//!
//! The game reads the profile save, then loads the main menu for it. A save last written with content that
//! is not there now (an addon's career, livery or item that the profile or Vanilla leaves out) can keep that
//! load from ever finishing: the spinner turns and team radio plays, with or without the launcher. The save is
//! encrypted, so the launcher learns such a save from the sessions that read it and then sat on the start
//! screen, and keys it by the SHA-256 of its data file, `savegame@profile#NXDSMWW.BWW`.
//!
//! ```text
//! $DATA/state/stalled-saves.json   the saves learned so far and where the launcher moved them from
//! ```
//!
//! Before Play and at launcher start, [`sweep`] finds a learned save in every save folder the launcher knows
//! (each profile's launcher saves, and the Steam or EA app save of the copy Play uses), keeps a snapshot of it
//! pinned [`PIN`], and records an [`Offer`] to restore the newest version the vault holds that is not a
//! learned one. The Career Saves page shows the offer. The sweep never swaps or empties a save; the player
//! decides. The launcher's copies between saves ([`refuse`]) never copy a learned save.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::game_folder::baseline::sha256_file;
use crate::saves::vault::restore::Running;
use crate::saves::vault::{EventKind, Layout, Location, Meta, Reason, Snapshot, Taken, Vault};
use crate::session::SessionRecord;
use crate::store::Store;
use crate::{Error, Result};

pub const FILE: &str = "state/stalled-saves.json";
/// The pin on the snapshot that holds a save moved aside.
pub const PIN: &str = "stalled-at-load";
/// Stalled sessions on the same save before it counts as one that stalls.
pub const SESSIONS: usize = 2;
/// How long a session must sit on the start screen after a profile load to count as stalled. Of the sessions
/// that reached the main menu, 99 in 100 left the start screen within 82 s.
pub const STALL_SECS: f64 = 120.0;
/// One stalled session at least this long is enough on its own; the slowest load that still reached the main
/// menu took 656 s.
pub const LONG_SECS: f64 = 900.0;
/// The screens F1 25 shows before the main menu: a session whose last screen is another one got past the load.
const BOOT_SCREENS: &[&str] = &[
    "persistent",
    "audio_themes",
    "tutorials_masked",
    "video_1",
    "legal",
    "legal_2",
    "seizure_warning",
    "splash",
    "render_badge",
    "splash_transition",
];
/// How many of the newest sessions a sweep reads.
const RECENT: usize = 60;
/// What the core logs for every profile load it sees.
const PROFILE_LOAD: &str = "[saves] profile load ";
const DATA_FILE: &str = "NXDSMWW.BWW";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub saves: Vec<Stalled>,
    #[serde(default)]
    pub moved: Vec<Moved>,
}

/// A save that kept the game on the start screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stalled {
    /// SHA-256 of the data file.
    pub sha256: String,
    pub size: u64,
    /// The sessions that stalled on it.
    pub sessions: Vec<String>,
    /// The longest any of them sat on the start screen, in seconds.
    pub secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub found_at: i64,
}

/// A learned save the sweep found in a save folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Moved {
    pub source: String,
    pub sha256: String,
    /// The snapshot that holds the moved save.
    #[serde(default)]
    pub kept: Option<String>,
    /// The snapshot put back in its place; set once the player restores.
    #[serde(default)]
    pub restored: Option<String>,
    /// The snapshot of the same vault the page offers to restore.
    #[serde(default)]
    pub offer: Option<String>,
    #[serde(default)]
    pub dismissed: bool,
    pub at: i64,
}

impl Ledger {
    pub fn path(data: &Path) -> PathBuf {
        data.join(FILE)
    }

    pub fn read(data: &Path) -> Self {
        crate::read_json(&Self::path(data)).unwrap_or_default()
    }

    pub fn write(&self, data: &Path) -> Result<()> {
        let path = Self::path(data);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
        }
        crate::write_json(&path, self)
    }

    pub fn stalled(&self, sha256: &str) -> Option<&Stalled> {
        self.saves.iter().find(|s| s.sha256 == sha256)
    }

    fn sizes(&self) -> impl Iterator<Item = u64> + '_ {
        self.saves.iter().map(|s| s.size)
    }

    fn moved_from(&self, source: &str, sha256: &str) -> bool {
        self.moved
            .iter()
            .any(|m| m.source == source && m.sha256 == sha256)
    }
}

/// One launcher session as the sweep reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub session: String,
    /// The data file's SHA-256 before the session.
    pub before: String,
    pub size: u64,
    /// The data file's SHA-256 after it, when the vault took one.
    pub after: Option<String>,
    pub profile: Option<String>,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// The game read the profile and sat on the start screen this many seconds until the session ended.
    Stalled(f64),
    /// The game read the profile and went on past the start screen.
    Passed,
    /// Neither can be told.
    Unknown,
}

/// The data file of a snapshot: `profile/NXDSMWW.BWW`, else the largest file.
pub fn data_entry(s: &Snapshot) -> Option<&crate::saves::vault::FileEntry> {
    s.files.iter().find(|f| f.name.ends_with(DATA_FILE))
}

/// The data file of the save in `loc`.
pub(crate) fn data_file(loc: &Location) -> Option<PathBuf> {
    let files = loc.files().ok()?;
    files
        .iter()
        .find(|f| f.key.ends_with(DATA_FILE))
        .map(|f| f.path.clone())
}

/// Whether the session's loader log has a profile load.
fn loaded_profile(dir: &Path) -> bool {
    ["loader.log", "game-logs/loader.log"]
        .iter()
        .map(|rel| dir.join(rel))
        .filter_map(|p| std::fs::File::open(p).ok())
        .any(|f| {
            BufReader::new(f)
                .split(b'\n')
                .map_while(std::result::Result::ok)
                .any(|l| {
                    l.windows(PROFILE_LOAD.len())
                        .any(|w| w == PROFILE_LOAD.as_bytes())
                })
        })
}

/// How the session in `dir` went past the profile load. A start-screen stall that ended in a crash (the game's
/// hang watchdog closes a frozen game) or that an addon known to cause one explains is not the save's.
pub fn outcome(dir: &Path, store: &Store) -> Outcome {
    let Some(perf) = crate::diagnostics::known_problems::read_perf(dir) else {
        return Outcome::Unknown;
    };
    match crate::diagnostics::known_problems::perf_start_screen(&perf) {
        Some(secs) if secs >= STALL_SECS => {
            let record = SessionRecord::read(dir);
            match !record.crashed()
                && loaded_profile(dir)
                && !crate::diagnostics::known_problems::addon_stall(&record, store)
            {
                true => Outcome::Stalled(secs),
                false => Outcome::Unknown,
            }
        }
        Some(_) => Outcome::Unknown,
        None => match crate::diagnostics::known_problems::last_screen(&perf) {
            Some(screen) if !BOOT_SCREENS.contains(&screen) => Outcome::Passed,
            Some(_) | None => Outcome::Unknown,
        },
    }
}

/// The vault snapshots each launcher session started and ended on, by session id.
fn session_snapshots(data: &Path) -> Vec<(String, Snapshot, Option<Snapshot>)> {
    let mut out = Vec::new();
    for source in Vault::sources(data) {
        let Ok(v) = Vault::open(data, &source) else {
            continue;
        };
        let snaps = v.list().unwrap_or_default();
        let find = |id: &Option<String>| {
            id.as_ref()
                .and_then(|id| snaps.iter().find(|s| &s.id == id))
                .cloned()
        };
        let events = v.events();
        for e in events.iter().filter(|e| e.kind == EventKind::SessionStart) {
            let (Some(session), Some(before)) = (e.detail.clone(), find(&e.snapshot)) else {
                continue;
            };
            let after = events
                .iter()
                .find(|x| x.kind == EventKind::SessionEnd && x.detail.as_ref() == Some(&session))
                .and_then(|x| find(&x.snapshot));
            out.push((session, before, after));
        }
    }
    out
}

/// A recent launcher session: its folder and the vault snapshots it started and ended on.
#[derive(Debug, Clone)]
pub(crate) struct Played {
    pub(crate) dir: PathBuf,
    pub(crate) before: Snapshot,
    pub(crate) after: Option<Snapshot>,
}

impl Played {
    /// The session's folder name.
    pub(crate) fn name(&self) -> String {
        self.dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
}

/// The newest [`RECENT`] session folders that the vault has a start snapshot for, newest first.
pub(crate) fn played(store: &Store) -> Vec<Played> {
    let snaps = session_snapshots(&store.root);
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(store.sessions_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs.iter().rev().take(RECENT) {
        let Some(id) = SessionRecord::read(dir).request_id else {
            continue;
        };
        let Some((_, before, after)) = snaps.iter().find(|(s, _, _)| *s == id) else {
            continue;
        };
        out.push(Played {
            dir: dir.clone(),
            before: before.clone(),
            after: after.clone(),
        });
    }
    out
}

/// The newest sessions with the save each read and how it went.
pub fn seen(store: &Store) -> Vec<Seen> {
    played(store)
        .into_iter()
        .filter_map(|p| {
            let entry = data_entry(&p.before)?;
            Some(Seen {
                session: p.name(),
                before: entry.sha256.clone(),
                size: entry.size,
                after: p.after.as_ref().and_then(data_entry).map(|f| f.sha256.clone()),
                profile: p.before.profile.clone(),
                outcome: outcome(&p.dir, store),
            })
        })
        .collect()
}

/// Fold the sessions into the ledger: a save stalls after [`SESSIONS`] stalled sessions, or one of
/// [`LONG_SECS`], and never once any session went past the start screen with it or the game wrote it.
pub fn learn(ledger: &mut Ledger, seen: &[Seen], now: i64) -> Vec<String> {
    let passed = |sha: &str| {
        seen.iter().any(|s| {
            s.before == sha
                && (s.outcome == Outcome::Passed || s.after.as_ref().is_some_and(|a| a != &s.before))
        })
    };
    ledger.saves.retain(|s| !passed(&s.sha256));
    let mut out = Vec::new();
    let mut shas: Vec<&str> = seen.iter().map(|s| s.before.as_str()).collect();
    shas.sort_unstable();
    shas.dedup();
    for sha in shas {
        if passed(sha) {
            continue;
        }
        let stalls: Vec<(&Seen, f64)> = seen
            .iter()
            .filter(|s| s.before == sha)
            .filter_map(|s| match s.outcome {
                Outcome::Stalled(secs) => Some((s, secs)),
                Outcome::Passed | Outcome::Unknown => None,
            })
            .collect();
        let longest = stalls.iter().map(|(_, secs)| *secs).fold(0.0, f64::max);
        if stalls.len() < SESSIONS && longest < LONG_SECS {
            continue;
        }
        let mut sessions: Vec<String> = stalls.iter().map(|(s, _)| s.session.clone()).collect();
        sessions.sort();
        let (first, _) = stalls[0];
        match ledger.saves.iter_mut().find(|s| s.sha256 == sha) {
            Some(known) => {
                for s in sessions {
                    if !known.sessions.contains(&s) {
                        known.sessions.push(s);
                    }
                }
                known.secs = known.secs.max(longest as u64);
            }
            None => {
                out.push(format!(
                    "saves: the profile save {} ({} bytes) kept F1 25 on Loading... after Press any button in {} session(s) ({}), up to {} s; \
                     the launcher no longer copies it and offers a restore on the Career Saves page",
                    short(sha),
                    first.size,
                    sessions.len(),
                    sessions.join(", "),
                    longest as u64
                ));
                ledger.saves.push(Stalled {
                    sha256: sha.to_owned(),
                    size: first.size,
                    sessions,
                    secs: longest as u64,
                    profile: first.profile.clone(),
                    found_at: now,
                });
            }
        }
    }
    out
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// The SHA-256 of the save in `loc` when it is a learned one.
pub fn stalled_in(ledger: &Ledger, loc: &Location) -> Option<String> {
    let path = data_file(loc)?;
    let size = std::fs::metadata(&path).ok()?.len();
    if !ledger.sizes().any(|s| s == size) {
        return None;
    }
    let sha = sha256_file(&path).ok()?;
    ledger.stalled(&sha).map(|_| sha)
}

/// The error a copy of a learned save gets.
pub fn refusal(loc: &Location) -> String {
    format!(
        "the profile save in {} kept F1 25 on Loading... after Press any button, so the launcher does not copy it; \
         the career vault keeps it to restore once the addons it was saved with are back",
        loc.dir.display()
    )
}

/// Refuse to copy the save in `from` when it is a learned one.
pub fn refuse(data: &Path, from: &Location) -> Result<()> {
    let ledger = Ledger::read(data);
    if ledger.saves.is_empty() {
        return Ok(());
    }
    match stalled_in(&ledger, from) {
        Some(_) => Err(Error::Other(refusal(from))),
        None => Ok(()),
    }
}

/// The vaults whose snapshots can go back into `loc`: its own first, then the game's saves for a launcher
/// store, the save it was copied from.
fn donors(data: &Path, loc: &Location) -> Vec<Vault> {
    let mut out: Vec<Vault> = Vault::of(data, loc).into_iter().collect();
    let prefix = match loc.layout {
        Layout::Launcher => Some("steam-"),
        Layout::LauncherEa => Some("ea-"),
        Layout::Steam | Layout::Ea => None,
    };
    if let Some(prefix) = prefix {
        out.extend(
            Vault::sources(data)
                .into_iter()
                .filter(|s| s.starts_with(prefix))
                .filter_map(|s| Vault::open(data, &s).ok()),
        );
    }
    out
}

/// The newest snapshot that can go back into `loc` and is not a learned save.
fn newest_good(data: &Path, loc: &Location, ledger: &Ledger) -> Option<(Vault, Snapshot)> {
    for v in donors(data, loc) {
        let good = v.list().unwrap_or_default().into_iter().find(|s| {
            s.layout.exchanges_with(loc.layout)
                && data_entry(s).is_some_and(|f| ledger.stalled(&f.sha256).is_none())
                && s.files.iter().all(|f| v.blob(&f.sha256).is_file())
        });
        if let Some(s) = good {
            return Some((v, s));
        }
    }
    None
}

/// A learned save in a save folder and the version the player can restore in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The vault source of the save folder holding the learned save.
    pub source: String,
    pub sha256: String,
    pub profile: Option<String>,
    pub secs: u64,
    pub sessions: usize,
    /// The snapshot that keeps the learned save, pinned [`PIN`].
    pub kept: Option<String>,
    /// The snapshot of the same vault that goes back on a restore.
    pub snapshot: Option<String>,
    pub snapshot_at: Option<i64>,
}

/// Pin a snapshot of the learned save in `loc` and record an offer. Nothing in the save folder changes.
/// The caller holds the session lock.
fn flag(data: &Path, loc: &Location, ledger: &mut Ledger, now: i64) -> Option<String> {
    let sha = stalled_in(ledger, loc)?;
    if ledger.moved_from(&loc.source, &sha) {
        return None;
    }
    let what = format!(
        "saves: the profile save in {} kept F1 25 on Loading... after Press any button",
        loc.dir.display()
    );
    let vault = match Vault::of(data, loc) {
        Ok(v) => v,
        Err(e) => return Some(format!("{what}; the career vault could not be opened: {e}")),
    };
    let kept = match vault.snapshot(
        loc,
        Meta::new(Reason::Manual)
            .pin(PIN)
            .note("kept F1 25 on Loading... after Press any button"),
        now,
    ) {
        Ok(Taken::New(s) | Taken::Unchanged(s)) => {
            if !s.pinned.iter().any(|p| p == PIN) {
                vault.pin(&s.id, PIN, now).ok();
            }
            Some(s.id)
        }
        Ok(Taken::NoSave) => return None,
        Err(e) => {
            return Some(format!(
                "{what}; the career vault could not keep a snapshot of it: {e}"
            ));
        }
    };
    let offer = newest_good(data, loc, ledger)
        .filter(|(v, _)| v.source == vault.source)
        .map(|(_, s)| s);
    ledger.moved.push(Moved {
        source: loc.source.clone(),
        sha256: sha,
        kept: kept.clone(),
        restored: None,
        offer: offer.as_ref().map(|s| s.id.clone()),
        dismissed: false,
        at: now,
    });
    Some(match offer {
        Some(good) => format!(
            "{what}; the career vault keeps a pinned snapshot of it ({}) and the Career Saves page offers the \
             version from {} ({}); the save is left as it is",
            kept.as_deref().unwrap_or("?"),
            crate::saves::vault::human(good.at),
            good.id
        ),
        None => format!(
            "{what}; the career vault keeps a pinned snapshot of it ({}) and holds no earlier version to offer; the \
             save is left as it is",
            kept.as_deref().unwrap_or("?")
        ),
    })
}

/// The learned saves still in their save folder that the player has not dismissed.
pub fn offers(data: &Path) -> Vec<Offer> {
    let ledger = Ledger::read(data);
    let active = Location::active(data);
    ledger
        .moved
        .iter()
        .filter(|m| !m.dismissed && m.restored.is_none())
        .filter_map(|m| {
            let loc = Location::of_source(data, &m.source)
                .or_else(|| active.clone().filter(|l| l.source == m.source))?;
            (stalled_in(&ledger, &loc).as_deref() == Some(m.sha256.as_str())).then_some(())?;
            let known = ledger.stalled(&m.sha256)?;
            let snapshot = m.offer.as_ref().and_then(|id| {
                let v = Vault::of(data, &loc).ok()?;
                v.find(id).ok()
            });
            Some(Offer {
                source: m.source.clone(),
                sha256: m.sha256.clone(),
                profile: known.profile.clone(),
                secs: known.secs,
                sessions: known.sessions.len(),
                kept: m.kept.clone(),
                snapshot_at: snapshot.as_ref().map(|s| s.at),
                snapshot: snapshot.map(|s| s.id),
            })
        })
        .collect()
}

/// Leave a learned save as it is: the page stops offering to restore it.
pub fn dismiss(data: &Path, source: &str, sha256: &str) -> Result<()> {
    let mut ledger = Ledger::read(data);
    let mut changed = false;
    for m in ledger
        .moved
        .iter_mut()
        .filter(|m| m.source == source && m.sha256 == sha256 && !m.dismissed)
    {
        m.dismissed = true;
        changed = true;
    }
    match changed {
        true => ledger.write(data),
        false => Ok(()),
    }
}

/// The save folders a sweep looks at: every profile's launcher saves and `game`, the save of the copy
/// Play uses.
fn locations(store: &Store, game: Option<&Location>) -> Vec<Location> {
    let data = &store.root;
    let mut out: Vec<Location> = Vec::new();
    for p in store.profile_names().unwrap_or_default() {
        out.push(crate::saves::launcher::location(data, &p));
        out.push(crate::saves::launcher::location_ea(data, &p));
    }
    out.extend(game.cloned());
    out.retain(crate::saves::launcher::has_files);
    out.dedup_by(|a, b| a.source == b.source);
    out
}

/// Learn from the newest sessions, then move every learned save out of the save folders. Returns the lines
/// to log. The caller holds the session lock.
pub fn sweep(store: &Store, game: Option<&Location>, _running: Running, now: i64) -> Vec<String> {
    let data = &store.root;
    let mut ledger = Ledger::read(data);
    let before = ledger.clone();
    let mut out = learn(&mut ledger, &seen(store), now);
    if !ledger.saves.is_empty() {
        for loc in locations(store, game) {
            out.extend(flag(data, &loc, &mut ledger, now));
        }
    }
    if ledger != before
        && let Err(e) = ledger.write(data)
    {
        out.push(format!(
            "saves: could not record the saves that stall at load: {e}"
        ));
    }
    out
}

#[cfg(test)]
mod tests;
