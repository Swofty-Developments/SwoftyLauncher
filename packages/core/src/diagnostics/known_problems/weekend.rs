//! The core's Team HQ weekend lines in a session's loader log, and the weekend a session ends stuck in: one that
//! started with no session run.

use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The core's Team HQ line for a weekend past not started with no session complete.
pub const NEVER_RAN: &str = "[career] weekend already started, session never ran: ";
/// The core's Team HQ line for every other weekend.
const TEAM_HQ: &str = "[career] Team HQ weekend: ";
/// A session load: the first flow state of a weekend session.
pub(super) const SESSION_LOAD: &str = "[career] flow \"StateLoadSession\" ";

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
