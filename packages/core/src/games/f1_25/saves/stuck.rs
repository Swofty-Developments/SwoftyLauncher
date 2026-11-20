//! Career saves written inside a race weekend whose sessions never ran.
//!
//! At every Team HQ entry the core logs the career's current weekend. A weekend past not started with no session
//! complete is logged as `weekend already started, session never ran`. A load of such a save opens Team HQ with no
//! Advance, and the weekend's first session never loads. The launcher learns such a save from a session whose last
//! Team HQ weekend line is that one, with no session load after it. It is keyed by the SHA-256 of the data file,
//! `savegame@profile#NXDSMWW.BWW`, of the save the session ended on.
//!
//! ```text
//! $DATA/state/stuck-weekends.json   the saves learned so far and the sessions already read
//! ```
//!
//! [`sweep`] runs at launcher start and after every Play. While a learned save is still in its save folder,
//! [`offers`] names the newest snapshot of the same vault that is not a learned save, and the Career Saves page
//! offers to restore it. Nothing here reads or changes the save itself; the player decides.

use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::game_folder::baseline::sha256_file;
use crate::saves::stall::{Played, data_entry, data_file, played};
use crate::saves::vault::{Location, Snapshot, Vault};
use crate::store::Store;

pub const FILE: &str = "state/stuck-weekends.json";
/// The core's Team HQ line for a weekend past not started with no session complete.
pub const NEVER_RAN: &str = "[career] weekend already started, session never ran: ";
/// The core's Team HQ line for every other weekend.
const TEAM_HQ: &str = "[career] Team HQ weekend: ";
/// A session load: the first flow state of a weekend session.
const SESSION_LOAD: &str = "[career] flow \"StateLoadSession\" ";

/// The weekend of a `never ran` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weekend {
    pub round: u32,
    pub rounds: u32,
    /// The current session, as the core names it (`practice_1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The track, as the core names it (`game/hungaroring`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
}

impl Weekend {
    /// The weekend of the text after [`NEVER_RAN`]: `round 12/22, weekend in progress, current session 1 of 7
    /// (practice_1), 0 of 7 sessions complete, track game/hungaroring`.
    pub fn parse(text: &str) -> Option<Self> {
        let (round, rest) = text.strip_prefix("round ")?.split_once('/')?;
        let (rounds, rest) = rest.split_once(',')?;
        let known = |s: &str| (!s.is_empty() && s != "unknown").then(|| s.to_owned());
        let session = rest
            .split_once("current session ")
            .and_then(|(_, r)| r.split_once(" (")?.1.split_once(')'))
            .and_then(|(s, _)| known(s));
        let track = rest.rsplit_once(", track ").and_then(|(_, t)| known(t.trim()));
        Some(Self {
            round: round.trim().parse().ok()?,
            rounds: rounds.trim().parse().ok()?,
            session,
            track,
        })
    }

    /// The track's own name: `hungaroring` for `game/hungaroring`.
    pub fn track_name(&self) -> Option<&str> {
        self.track.as_deref().map(|t| t.rsplit('/').next().unwrap_or(t))
    }

    /// `round 12 of 22 (hungaroring)`.
    pub fn text(&self) -> String {
        let place = format!("round {} of {}", self.round, self.rounds);
        match self.track_name() {
            Some(t) => format!("{place} ({t})"),
            None => place,
        }
    }
}

/// Reads a loader log line by line and keeps the weekend it ends stuck in.
#[derive(Debug, Default)]
struct Scan {
    stuck: Option<Weekend>,
}

impl Scan {
    fn line(&mut self, line: &str) {
        if let Some(i) = line.find(NEVER_RAN) {
            self.stuck = Weekend::parse(&line[i + NEVER_RAN.len()..]);
        } else if line.contains(TEAM_HQ) || line.contains(SESSION_LOAD) {
            self.stuck = None;
        }
    }
}

/// The weekend a session's loader log ends stuck in: its last Team HQ weekend line is a [`NEVER_RAN`] one, and no
/// session load comes after it.
pub fn stuck_weekend(loader: &str) -> Option<Weekend> {
    let mut scan = Scan::default();
    loader.lines().for_each(|l| scan.line(l));
    scan.stuck
}

/// [`stuck_weekend`] of the loader log in the session folder `dir`, read line by line.
pub fn stuck_in(dir: &Path) -> Option<Weekend> {
    let path = ["loader.log", "game-logs/loader.log"]
        .iter()
        .map(|rel| dir.join(rel))
        .find(|p| p.is_file())?;
    let mut scan = Scan::default();
    BufReader::new(std::fs::File::open(path).ok()?)
        .split(b'\n')
        .map_while(std::result::Result::ok)
        .for_each(|l| scan.line(&String::from_utf8_lossy(&l)));
    scan.stuck
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub saves: Vec<Stuck>,
    /// The session folders already read.
    #[serde(default)]
    pub read: Vec<String>,
}

/// A save that holds a career stuck in a started weekend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stuck {
    /// The vault source of the save folder the save was in.
    pub source: String,
    /// SHA-256 of the data file.
    pub sha256: String,
    pub size: u64,
    pub weekend: Weekend,
    /// The sessions that ended stuck on it.
    pub sessions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub found_at: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dismissed: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Ledger {
    pub fn path(data: &Path) -> std::path::PathBuf {
        data.join(FILE)
    }

    pub fn read(data: &Path) -> Self {
        crate::read_json(&Self::path(data)).unwrap_or_default()
    }

    pub fn write(&self, data: &Path) -> Result<()> {
        crate::write_json_shared(&Self::path(data), self)
    }

    /// Whether any learned save has this data file.
    pub fn holds(&self, sha256: &str) -> bool {
        self.saves.iter().any(|s| s.sha256 == sha256)
    }
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// Fold the sessions into the ledger: each session not read yet whose vault holds its end snapshot is read once,
/// and one that ended stuck makes the save it ended on a learned one. Returns the lines to log.
pub(crate) fn learn(ledger: &mut Ledger, played: &[Played], now: i64) -> Vec<String> {
    let mut out = Vec::new();
    for p in played {
        let name = p.name();
        let Some(after) = p.after.as_ref() else {
            continue;
        };
        if ledger.read.contains(&name) {
            continue;
        }
        ledger.read.push(name.clone());
        let Some(weekend) = stuck_in(&p.dir) else {
            continue;
        };
        let Some(entry) = data_entry(after) else {
            continue;
        };
        match ledger
            .saves
            .iter_mut()
            .find(|s| s.source == after.source && s.sha256 == entry.sha256)
        {
            Some(known) => {
                if !known.sessions.contains(&name) {
                    known.sessions.push(name);
                    known.sessions.sort();
                }
            }
            None => {
                out.push(format!(
                    "saves: session {name} ended with its career in the {} weekend, started with no session run; \
                     the profile save {} ({} bytes) of {} holds it, and the Career Saves page offers an earlier \
                     version",
                    weekend.text(),
                    short(&entry.sha256),
                    entry.size,
                    after.source
                ));
                ledger.saves.push(Stuck {
                    source: after.source.clone(),
                    sha256: entry.sha256.clone(),
                    size: entry.size,
                    weekend,
                    sessions: vec![name],
                    profile: after.profile.clone(),
                    found_at: now,
                    dismissed: false,
                });
            }
        }
    }
    let window: Vec<String> = played.iter().map(Played::name).collect();
    ledger.read.retain(|n| window.contains(n));
    out
}

/// Learn from the newest sessions. Returns the lines to log.
pub fn sweep(store: &Store, now: i64) -> Vec<String> {
    let data = &store.root;
    let mut ledger = Ledger::read(data);
    let before = ledger.clone();
    let mut out = learn(&mut ledger, &played(store), now);
    if ledger != before
        && let Err(e) = ledger.write(data)
    {
        out.push(format!(
            "saves: could not record the saves stuck in a started weekend: {e}"
        ));
    }
    out
}

/// The SHA-256 of the data file in `loc` when its size is `size`.
fn current(loc: &Location, size: u64) -> Option<String> {
    let path = data_file(loc)?;
    (std::fs::metadata(&path).ok()?.len() == size).then_some(())?;
    sha256_file(&path).ok()
}

/// The newest snapshot of `vault` that can go back into `loc` and is not a learned save.
fn newest_unstuck(vault: &Vault, loc: &Location, ledger: &Ledger) -> Option<Snapshot> {
    vault.list().unwrap_or_default().into_iter().find(|s| {
        s.layout.exchanges_with(loc.layout)
            && data_entry(s).is_some_and(|f| !ledger.holds(&f.sha256))
            && s.files.iter().all(|f| vault.blob(&f.sha256).is_file())
    })
}

/// A learned save still in its save folder and the version the player can restore in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The vault source of the save folder holding the learned save.
    pub source: String,
    pub sha256: String,
    pub profile: Option<String>,
    pub weekend: Weekend,
    pub sessions: usize,
    /// The snapshot of the same vault that goes back on a restore.
    pub snapshot: Option<String>,
    pub snapshot_at: Option<i64>,
}

/// The learned saves still in their save folder that the player has not dismissed.
pub fn offers(data: &Path) -> Vec<Offer> {
    let ledger = Ledger::read(data);
    let mut active: Option<Option<Location>> = None;
    let mut out = Vec::new();
    for s in ledger.saves.iter().filter(|s| !s.dismissed) {
        let loc = match Location::of_source(data, &s.source) {
            Some(l) => l,
            None => match active
                .get_or_insert_with(|| Location::active(data))
                .clone()
                .filter(|l| l.source == s.source)
            {
                Some(l) => l,
                None => continue,
            },
        };
        if current(&loc, s.size).as_deref() != Some(s.sha256.as_str()) {
            continue;
        }
        let snapshot = Vault::of(data, &loc)
            .ok()
            .and_then(|v| newest_unstuck(&v, &loc, &ledger));
        out.push(Offer {
            source: s.source.clone(),
            sha256: s.sha256.clone(),
            profile: s.profile.clone(),
            weekend: s.weekend.clone(),
            sessions: s.sessions.len(),
            snapshot_at: snapshot.as_ref().map(|s| s.at),
            snapshot: snapshot.map(|s| s.id),
        });
    }
    out
}

/// Leave a learned save as it is: the page stops offering to restore it.
pub fn dismiss(data: &Path, source: &str, sha256: &str) -> Result<()> {
    let mut ledger = Ledger::read(data);
    let mut changed = false;
    for s in ledger
        .saves
        .iter_mut()
        .filter(|s| s.source == source && s.sha256 == sha256 && !s.dismissed)
    {
        s.dismissed = true;
        changed = true;
    }
    match changed {
        true => ledger.write(data),
        false => Ok(()),
    }
}

#[cfg(test)]
mod tests;
