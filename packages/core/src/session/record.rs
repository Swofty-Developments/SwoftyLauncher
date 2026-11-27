//! What a session leaves in its log folder: how the game ended (`end.json`), the session's own record, and the loader log mark that tells whose log it is.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::STEAM_APP_ID;

use super::{LOADER_FALLBACK, LOADER_LOG, LOADER_MARK};

pub const END_FILE: &str = "end.json";
pub const STOP_FILE: &str = "stop_requested";

/// How a session's game ended. `Crash` needs evidence: a crash exit code,
/// or an unhandled exception the mod loader saw. F1 25's own crash handler
/// ends the game with -1, a code it also leaves when it fails while
/// closing, so -1 alone is `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ending {
    Quit,
    Crash,
    Stopped,
    Unknown,
}

impl Ending {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Crash => "crash",
            Self::Stopped => "stopped",
            Self::Unknown => "unknown",
        }
    }
}

/// An exit code a crash leaves: an error NTSTATUS (0xC0000000 and up) other
/// than -1, or an unhandled breakpoint.
/// The exit code the core closes F1 25 with when it stops a Grand Prix or Driver Career load that would crash
/// (native/src/core/boxes/era_logic.hpp `CONTINUE_STOP_EXIT`).
pub const CORE_STOPPED_LOAD: i32 = 92;

pub fn crash_code(code: u32) -> bool {
    (0xc000_0000..0xffff_ffff).contains(&code) || code == 0x8000_0003
}

/// The session log's `end.json`: how the game ended, its exit code when the
/// session could read it, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEnd {
    pub end: Ending,
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crash_code: Option<u32>,
    pub reason: String,
}

impl SessionEnd {
    pub fn write(&self, dir: &Path) -> crate::Result<()> {
        crate::write_json(&dir.join(END_FILE), self)
    }
}

/// Mark the live session as stopped by the player before its game is
/// closed, so its end is reported as a stop and not a crash.
pub fn request_stop(data: &Path) {
    let Ok(dir) = std::fs::read_to_string(data.join("sessions/latest.txt")) else {
        return;
    };
    let session = SessionRecord::read(Path::new(dir.trim()));
    if session.started.is_some() && session.ended.is_none() {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        std::fs::write(session.dir.join(STOP_FILE), format!("{now}\n")).ok();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub dir: PathBuf,
    pub request_id: Option<String>,
    pub started: Option<f64>,
    pub ended: Option<f64>,
    pub exit_code: Option<i32>,
    pub end: Option<SessionEnd>,
}

impl SessionRecord {
    pub fn read(dir: &Path) -> Self {
        let read = |f: &str| {
            std::fs::read_to_string(dir.join(f))
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        Self {
            dir: dir.into(),
            request_id: read("request_id"),
            started: read("game_start").and_then(|s| s.parse().ok()),
            ended: read("game_end").and_then(|s| s.parse().ok()),
            exit_code: read("exit_code").and_then(|s| s.parse().ok()),
            end: crate::read_json(&dir.join(END_FILE)).ok(),
        }
    }

    /// Whether to show the session as a crash: only an end with evidence of
    /// one, a crash exit code, an exception the mod loader saw or the game's
    /// Insufficient Storage dialog. F1 25 also
    /// leaves -1 and other codes when it is closed, so an `Unknown` end shows
    /// as closed, not crashed.
    pub fn crashed(&self) -> bool {
        match &self.end {
            Some(e) => e.end == Ending::Crash,
            None => self.exit_code.is_some_and(|c| crash_code(c as u32)),
        }
    }

    pub fn wine_log(&self) -> PathBuf {
        self.dir.join(format!("steam-{STEAM_APP_ID}.log"))
    }

    pub fn owns_loader_log(&self, log: &Path) -> bool {
        let Some(before) = std::fs::read(self.dir.join(LOADER_MARK))
            .ok()
            .and_then(|d| serde_json::from_slice::<Option<LogMark>>(&d).ok())
        else {
            return false;
        };
        LogMark::replaced(log, before.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogMark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inode: Option<u64>,
    pub head: String,
}

impl LogMark {
    pub fn read(path: &Path) -> Option<Self> {
        let file = std::fs::File::open(path).ok()?;
        #[cfg(unix)]
        let inode = Some(std::os::unix::fs::MetadataExt::ino(&file.metadata().ok()?));
        #[cfg(not(unix))]
        let inode = None;
        let mut head = Vec::new();
        file.take(512).read_to_end(&mut head).ok()?;
        let end = head.iter().position(|&b| b == b'\n').unwrap_or(head.len());
        Some(Self {
            inode,
            head: String::from_utf8_lossy(&head[..end]).trim_end().to_owned(),
        })
    }

    pub fn replaced(path: &Path, before: Option<&Self>) -> bool {
        Self::read(path).is_some_and(|now| before != Some(&now))
    }
}

/// Where this session's loader log is: the game folder's `loader.log` once this session's game has replaced the
/// one from before the launch, else the fallback in the data folder, which a loader the game folder refuses
/// writes instead. The fallback counts only when it was written since the launch and, if it names a launch
/// request, names this session's, so an earlier session's fallback is never taken for this one.
#[derive(Debug, Clone)]
pub struct LoaderLog {
    game: PathBuf,
    before: Option<LogMark>,
    fallback: PathBuf,
    request: Option<String>,
    launched: SystemTime,
}

const FALLBACK_HEAD: u64 = 4096;
const CLOCK_SLACK: Duration = Duration::from_secs(2);
const REQUEST_MARK: &str = "launch request ";

impl LoaderLog {
    pub fn new(
        game: &Path,
        before: Option<LogMark>,
        data: &Path,
        request: Option<String>,
        launched: SystemTime,
    ) -> Self {
        Self {
            game: game.join(LOADER_LOG),
            before,
            fallback: data.join(LOADER_FALLBACK),
            request: request.filter(|r| !r.is_empty()),
            launched,
        }
    }

    pub fn game_log(&self) -> &Path {
        &self.game
    }

    pub fn current(&self) -> Option<PathBuf> {
        if LogMark::replaced(&self.game, self.before.as_ref()) {
            return Some(self.game.clone());
        }
        self.fallback_is_ours().then(|| self.fallback.clone())
    }

    fn fallback_is_ours(&self) -> bool {
        if !written_since(&self.fallback, self.launched - CLOCK_SLACK) {
            return false;
        }
        let Some(want) = &self.request else {
            return true;
        };
        let mut head = Vec::new();
        let read =
            std::fs::File::open(&self.fallback).and_then(|f| f.take(FALLBACK_HEAD).read_to_end(&mut head));
        if read.is_err() {
            return false;
        }
        let head = String::from_utf8_lossy(&head);
        match head.find(REQUEST_MARK) {
            Some(at) => head[at + REQUEST_MARK.len()..]
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .is_some_and(|id| id == want),
            None => true,
        }
    }
}

pub fn written_since(path: &Path, since: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t >= since)
}
