//! The crash reports and minidumps the core wrote, newest first, and the lines of a report that say what
//! crashed.

use std::path::{Path, PathBuf};

use super::collect::Sources;
use super::files::{file_name, modified, read_head, read_tail};
use super::{LOADER_DIR, PERF_READ, size};
use crate::session::LOADER_FALLBACK;
use crate::store::Store;

/// A crash report the core wrote (`crash-<time>.txt`) or its minidump
/// (`crash-<time>.dmp`).
pub(super) fn is_crash_file(path: &Path) -> bool {
    crate::diagnostics::crash_report::is_crash_name(&file_name(path))
}

pub(super) enum CrashFound<'a> {
    Report(&'a str),
    Unsaved(String),
    FastFail(Vec<String>),
    None,
}

pub(super) const LOGGED_READ: u64 = 4 << 20;

/// The crash report path the newest session's loader.log (or the fallback log in the data folder) says the
/// core began writing, for when no such file turned up.
impl Sources<'_> {
    pub(super) fn logged_report(&self) -> Option<String> {
        let mut logs: Vec<PathBuf> = self
            .sessions
            .first()
            .map(|d| d.join("loader.log"))
            .into_iter()
            .collect();
        logs.push(self.store.root.join(LOADER_FALLBACK));
        logs.iter().find_map(|log| {
            let (tail, _) = read_tail(log, LOGGED_READ).ok()?;
            crate::diagnostics::crash_report::logged_path(&String::from_utf8_lossy(&tail))
        })
    }

    /// The `n` newest crash reports for this bundle, as [`recent_crashes`] finds them.
    pub(super) fn crashes(&self, n: usize) -> Vec<Crash> {
        recent_crashes(self.store, self.game, &self.sessions, n)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Crash {
    pub report: PathBuf,
    pub dump: Option<PathBuf>,
}

/// The newest crash report across the running game's loader folder, the
/// data folder's fallback and the kept sessions, with its minidump beside it when one exists.
pub fn newest_crash(store: &Store, game: Option<&Path>, sessions: &[PathBuf]) -> Option<Crash> {
    recent_crashes(store, game, sessions, 1).into_iter().next()
}

/// The `n` newest crash reports from the same folders, newest first, each with its minidump when one
/// exists. A report copied into a kept session counts once.
pub fn recent_crashes(store: &Store, game: Option<&Path>, sessions: &[PathBuf], n: usize) -> Vec<Crash> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(game) = game {
        dirs.push(game.join(LOADER_DIR));
    }
    for s in sessions {
        dirs.push(s.clone());
        dirs.push(s.join("game-logs"));
    }
    dirs.push(crate::diagnostics::crash_report::fallback_dir(&store.root));
    let mut reports: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| {
            let n = file_name(p);
            crate::diagnostics::crash_report::is_crash_name(&n) && n.ends_with(".txt") && p.is_file()
        })
        .collect();
    // The names carry a sortable UTC-ish stamp, so the last by name is the newest; modified time
    // breaks ties and covers a clock change.
    reports.sort_by(|a, b| {
        modified(b)
            .cmp(&modified(a))
            .then_with(|| file_name(b).cmp(&file_name(a)))
    });
    let mut seen = std::collections::HashSet::new();
    reports
        .into_iter()
        .filter(|p| seen.insert(file_name(p)))
        .take(n)
        .map(|report| {
            let dump = report.with_extension("dmp");
            Crash {
                dump: dump.is_file().then_some(dump),
                report,
            }
        })
        .collect()
}

/// One line per crash report found: its name and size, its minidump and where in the bundle it went.
pub(super) fn dumps_listing(crashes: &[Crash]) -> Option<String> {
    if crashes.is_empty() {
        return None;
    }
    let mut out = String::from("Crash reports and minidumps found, newest first\n");
    for (i, c) in crashes.iter().enumerate() {
        let len = |p: &Path| std::fs::metadata(p).map_or_else(|_| "unreadable".into(), |m| size(m.len()));
        let dump = match &c.dump {
            Some(d) => format!("{} {}", file_name(d), len(d)),
            None => "no minidump".into(),
        };
        let sent = if i == 0 {
            "report and minidump in crash/"
        } else {
            "report in crash/earlier/"
        };
        out.push_str(&format!(
            "  {} {}; {dump}; {sent}\n",
            file_name(&c.report),
            len(&c.report)
        ));
    }
    Some(out)
}

/// A short summary of the crash report at `path`, for `swofty logs`.
pub fn summarise_crash(path: &Path) -> std::io::Result<Vec<String>> {
    Ok(crash_excerpt(&String::from_utf8_lossy(&read_head(
        path, PERF_READ,
    )?)))
}

/// The head of a crash report, cut to its most useful lines: the header
/// fields, the exception and the first stack frames.
pub(super) fn crash_excerpt(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut section = "";
    let mut stack = 0;
    let kept = |s: &str| {
        s == "Exception"
            || s == "Ne heap"
            || s == "Game state (from the black box)"
            || s.starts_with("Stack (")
    };
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("== ") {
            section = name;
            if kept(section) {
                out.push(String::new());
                out.push(line.to_owned());
            }
            continue;
        }
        if out.len() > 400 {
            break;
        }
        if section.is_empty() {
            // The header block, up to the blank line before the first section.
            if line.is_empty() {
                continue;
            }
            out.push(line.to_owned());
        } else if section == "Exception"
            || section == "Ne heap"
            || section == "Game state (from the black box)"
        {
            out.push(line.to_owned());
        } else if section.starts_with("Stack (") && stack < 12 {
            out.push(line.to_owned());
            stack += 1;
        }
    }
    out
}
