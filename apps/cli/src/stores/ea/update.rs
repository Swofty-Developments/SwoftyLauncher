//! A launch link makes the EA app install a pending game update before it
//! starts F1 25, after the session built and installed the profile for the
//! version before. The update puts its own F1_25.exe and game database over
//! the session's, so the game would start on the stock database with addons
//! built for the old one.

use std::path::Path;
use std::time::SystemTime;

use swofty_core::GAME_EXE;
use swofty_core::build::GVDB;
use swofty_core::platform::process::kill_process;

use crate::console::output::say;

const WATCHED: [&str; 2] = [GAME_EXE, GVDB];

pub(crate) const CLOSED: &str = "the EA app updated F1 25 as it started it, so the session closed it to build the profile for the new version";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

/// F1_25.exe and the game database as they were when the launch link went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed([Option<Stamp>; 2]);

impl Installed {
    pub(crate) fn read(game: &Path) -> Self {
        Self(WATCHED.map(|rel| stamp(&game.join(rel))))
    }

    /// The watched files another program wrote since `self` was read.
    pub(crate) fn replaced(&self, game: &Path) -> Vec<&'static str> {
        let now = Self::read(game);
        WATCHED
            .iter()
            .zip(self.0.iter().zip(now.0.iter()))
            .filter(|(_, (was, is))| was != is)
            .map(|(rel, _)| *rel)
            .collect()
    }
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp {
        len: meta.len(),
        modified: meta.modified().ok(),
    })
}

/// The session's game started on an update's files; the wrapper starts it
/// again on a profile built for the new version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Updated {
    pub(crate) files: Vec<&'static str>,
}

impl std::fmt::Display for Updated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the EA app updated F1 25 as it started it ({} replaced)",
            self.files.join(" and ")
        )
    }
}

impl std::error::Error for Updated {}

/// Close the game `pid` before it reads the database when an update replaced
/// what the session installed. Only a session that installed the profile has
/// files an update can put the game out of step with.
pub(crate) fn close_if_updated(pid: u32, replaced: &[&'static str], applied: bool) -> Option<Updated> {
    if !applied || replaced.is_empty() {
        return None;
    }
    let updated = Updated {
        files: replaced.to_vec(),
    };
    say(
        "ea app",
        &format!(
            "{updated} after the launch link, over the files built for the version before; closing it (pid {pid}) \
             before it loads them"
        ),
    );
    if let Err(e) = kill_process(pid) {
        say("ea app", &format!("could not close F1 25 (pid {pid}): {e}"));
    }
    Some(updated)
}
