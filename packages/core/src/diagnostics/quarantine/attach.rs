//! Watching loader.log after launch for version.dll attaching, or being refused.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::session::LogMark;

/// The loader's first line once version.dll is running in the game.
pub const ATTACHED: &str = "(version.dll proxy) attached";
/// The tail of the attach line that survives when a concurrent writer
/// overwrites the start of it.
pub(super) const ATTACHED_TAIL: &str = "proxy) attached";
/// Lines only the loader writes, which show it ran when a concurrent writer
/// (the offline play stub) has overwritten the attach line itself: its
/// preamble lines and the core's output.
pub(super) const LOADER_LINES: &[&str] = &["] real version.dll: ", "] forwards: ", "[core] "];
/// The offline play stub's line when it has the game image and resumes it.
pub(super) const PATCHING: &str = "game image at ";
/// The loader's line in its fallback log naming the game folder's log it
/// could not open.
pub(super) const REFUSED: &str = " could not be opened (error ";
/// A second look at the placed DLLs this long after they were written: a
/// scanner often removes a file only once it has been closed.
pub const RECHECK_AFTER: Duration = Duration::from_secs(2);
/// How long after the stub's patching line the loader may take to attach.
pub const ATTACH_WINDOW: Duration = Duration::from_secs(45);
/// The window when no stub line shows up (a launch without offline play).
pub(super) const ATTACH_WINDOW_NO_STUB: Duration = Duration::from_secs(90);
/// A game that exits sooner than this without the loader failed to start,
/// which the crash report covers.
pub(super) const SHORTEST_RUN: Duration = Duration::from_secs(10);
pub(super) const POLL: Duration = Duration::from_secs(1);
/// How long after the stub's patching line the session looks inside the game
/// for the loader, while the game is still running: a game that closes within
/// the attach window is gone by the time the verdict comes.
pub const PROBE_AFTER: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attach {
    Waiting,
    Attached,
    Missed,
}

/// Whether a loader.log shows the loader running: the attach line anywhere
/// in it, or, when the stub's late lines overwrote that line, any other line
/// only the loader writes. `Some(false)` for a log with lines but none of
/// the loader's (only the offline play stub's), `None` for an empty one.
pub fn attached(text: &str) -> Option<bool> {
    if text.contains(ATTACHED_TAIL) || LOADER_LINES.iter().any(|l| text.contains(l)) {
        return Some(true);
    }
    (!text.trim().is_empty()).then_some(false)
}

/// Why the loader wrote its fallback log: the game folder's log it could not
/// open, with the error, and the rights the game runs with.
pub fn refused(fallback: &str) -> Option<String> {
    let mut lines = fallback.lines();
    let log = lines.find(|l| l.contains(REFUSED))?;
    let log = &log[log.find("log: ")? + "log: ".len()..];
    let log = &log[..log.find(';').unwrap_or(log.len())];
    let rights = lines
        .find_map(|l| Some(&l[l.find("process: ")? + "process: ".len()..]))
        .map(|p| format!("; the game runs as {}", p.trim_end()))
        .unwrap_or_default();
    Some(format!("{log}{rights}"))
}

/// What the session says when the loader attached but logs to its fallback.
/// The EA app can start F1 25 without administrator rights even when an
/// elevated launcher started the EA app, so it names no restart as the fix.
pub fn refused_text() -> &'static str {
    "F1 25 can't create files in its folder, so this session the loader logs to the data folder and addons keep \
     their files in addon-storage in the data folder. An addon that writes in the game folder by itself can't save \
     there; moving F1 25 out of Program Files with EA app > F1 25 > Manage > Move, not File Explorer, fixes that."
}

/// Follows the game folder's loader.log after the game starts. The stub
/// writes its lines first; the loader moves them to loader.prev.log when it
/// attaches and starts a fresh log with its attach line, so a log still
/// equal to the one from before the launch says nothing. A loader the game
/// folder refuses logs to the fallback in the data folder instead, which a
/// session starts without.
pub struct AttachWatch {
    log: PathBuf,
    before: Option<LogMark>,
    fallback: Option<PathBuf>,
    started: Instant,
    patched: Option<Instant>,
    probed: bool,
    next: Instant,
    state: Attach,
}

impl AttachWatch {
    pub fn new(log: PathBuf, before: Option<LogMark>, now: Instant) -> Self {
        Self {
            log,
            before,
            fallback: None,
            started: now,
            patched: None,
            probed: false,
            next: now,
            state: Attach::Waiting,
        }
    }

    pub fn with_fallback(self, fallback: PathBuf) -> Self {
        Self {
            fallback: Some(fallback),
            ..self
        }
    }

    pub fn state(&self) -> Attach {
        self.state
    }

    /// The state after a look at the log; a verdict, once reached, stays.
    pub fn poll(&mut self, now: Instant) -> Attach {
        if self.state != Attach::Waiting || now < self.next {
            return self.state;
        }
        self.next = now + POLL;
        if self.read(now) == Some(true) {
            self.state = Attach::Attached;
            return self.state;
        }
        let (from, window) = match self.patched {
            Some(at) => (at, ATTACH_WINDOW),
            None => (self.started, ATTACH_WINDOW_NO_STUB),
        };
        if now.duration_since(from) > window {
            self.state = Attach::Missed;
        }
        self.state
    }

    /// True once, when the game has run `PROBE_AFTER` since the stub patched
    /// it and the loader has not attached: the moment to look at which
    /// version.dll it holds.
    pub fn probe_due(&mut self, now: Instant) -> bool {
        if self.probed || self.state != Attach::Waiting {
            return false;
        }
        let Some(at) = self.patched else {
            return false;
        };
        if now.duration_since(at) < PROBE_AFTER {
            return false;
        }
        self.probed = true;
        self.read(now) != Some(true)
    }

    /// The verdict when the game has exited: a game that ran a while and
    /// never got the loader missed it.
    pub fn ended(&mut self, now: Instant) -> Attach {
        if self.state == Attach::Waiting {
            self.state = if self.read(now) == Some(true) {
                Attach::Attached
            } else if now.duration_since(self.started) >= SHORTEST_RUN {
                Attach::Missed
            } else {
                Attach::Waiting
            };
        }
        self.state
    }

    fn read(&mut self, now: Instant) -> Option<bool> {
        if let Some(fallback) = self.fallback.as_ref().and_then(|f| std::fs::read(f).ok())
            && attached(&String::from_utf8_lossy(&fallback)) == Some(true)
        {
            return Some(true);
        }
        let text = std::fs::read(&self.log).ok()?;
        let text = String::from_utf8_lossy(&text);
        if self.patched.is_none() && text.contains(PATCHING) {
            self.patched = Some(now);
        }
        if !LogMark::replaced(&self.log, self.before.as_ref()) {
            return None;
        }
        attached(&text)
    }
}
