use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use swofty_addon::launcher::GameStatus;

use crate::GAME_EXE;
use crate::session::SessionRecord;

mod elevation;
mod image;
mod list;
mod paths;
mod times;

pub use elevation::{ExitProbe, elevation, own_elevation};
use image::origin;
pub use list::descends_from;
pub(crate) use list::named;
#[cfg(windows)]
pub(crate) use list::named_paths;
use paths::{exe_folder, host_path};
pub use paths::{same_folder, same_path};
#[cfg(any(target_os = "linux", windows))]
pub(crate) use times::started;

#[derive(Debug, Clone, PartialEq)]
pub struct GameProcess {
    pub pid: u32,
    pub started: Option<SystemTime>,
    pub origin: Origin,
}

impl GameProcess {
    /// Whether this is the game in `dir`. A process that tells nothing about
    /// where it was started from belongs to no copy.
    pub fn is_in(&self, dir: &Path) -> bool {
        self.origin.folders().iter().any(|f| same_folder(f, dir))
    }

    /// Whether this is the game of a folder other than `dir`.
    pub fn elsewhere(&self, dir: &Path) -> bool {
        let folders = self.origin.folders();
        !folders.is_empty() && !folders.iter().any(|f| same_folder(f, dir))
    }
}

/// What a running F1_25.exe tells about which copy of the game it is: the
/// exe file (the process image on Windows, the file mapped at the image base
/// under Wine), and under Wine the Windows path it was started with and the
/// prefix that maps that path's drive.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Origin {
    pub image: Option<String>,
    pub command: Option<String>,
    pub prefix: Option<PathBuf>,
}

impl Origin {
    pub fn folders(&self) -> Vec<PathBuf> {
        let image = self.image.as_deref().and_then(exe_folder);
        let command = self
            .command
            .as_deref()
            .and_then(|c| host_path(c, self.prefix.as_deref()))
            .and_then(|p| exe_folder(&p.to_string_lossy()));
        image.into_iter().chain(command).collect()
    }
}

/// `--<name>=<value>` as one argument, so a value that starts with `-`, such as
/// a profile named `-swofty`, is not read as another flag.
pub fn flag(name: &str, value: impl AsRef<std::ffi::OsStr>) -> std::ffi::OsString {
    let mut arg = std::ffi::OsString::from(format!("--{name}="));
    arg.push(value);
    arg
}

pub fn game_running() -> bool {
    !game_processes().is_empty()
}

/// The running games of the copy in `dir`.
pub fn games_in(dir: &Path) -> Vec<GameProcess> {
    game_processes().into_iter().filter(|g| g.is_in(dir)).collect()
}

pub fn game_running_in(dir: &Path) -> bool {
    !games_in(dir).is_empty()
}

/// The running game for a launcher plugin's game_status: its process, and the
/// latest swofty session under `data` while that session is still live.
pub fn game_status(data: Option<&Path>) -> GameStatus {
    status_of(&game_processes(), data)
}

fn status_of(processes: &[GameProcess], data: Option<&Path>) -> GameStatus {
    let Some(first) = processes.first() else {
        return GameStatus::default();
    };
    let session = data
        .and_then(|d| std::fs::read_to_string(d.join("sessions/latest.txt")).ok())
        .map(|dir| SessionRecord::read(Path::new(dir.trim())))
        .filter(|s| s.started.is_some() && s.ended.is_none())
        .and_then(|s| Some(s.dir.file_name()?.to_string_lossy().into_owned()));
    GameStatus {
        running: true,
        pid: Some(first.pid),
        since: first
            .started
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64()),
        session,
    }
}

pub fn game_processes() -> Vec<GameProcess> {
    // A process's exe never changes, and reading a running game's maps
    // takes its memory-map lock, so a game's origin is read once under Wine.
    static SEEN: std::sync::Mutex<Vec<(u32, Origin)>> = std::sync::Mutex::new(Vec::new());
    let found = named(GAME_EXE);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.retain(|(pid, _)| found.iter().any(|g| g.pid == *pid));
    found
        .into_iter()
        .map(|g| {
            let origin = match seen.iter().find(|(pid, _)| *pid == g.pid) {
                Some((_, o)) => o.clone(),
                None => {
                    let o = origin(g.pid);
                    if cfg!(target_os = "linux") && o.image.is_some() {
                        seen.push((g.pid, o.clone()));
                    }
                    o
                }
            };
            GameProcess { origin, ..g }
        })
        .collect()
}

/// The games in `running` that were not there before a launch (`before`),
/// that the session did not start (`ours`) and that are not another copy's
/// than the one in `dir`: the EA App build hands a launch back to the EA
/// app, which starts the game again outside the session.
pub fn started_elsewhere(
    running: &[GameProcess],
    dir: &Path,
    before: &[u32],
    ours: impl Fn(u32) -> bool,
) -> Vec<u32> {
    running
        .iter()
        .filter(|g| !before.contains(&g.pid) && !ours(g.pid) && !g.elsewhere(dir))
        .map(|g| g.pid)
        .collect()
}

pub fn steamvr_running() -> bool {
    !named(if cfg!(windows) { "vrserver.exe" } else { "vrserver" }).is_empty()
}

pub fn stop_process(pid: u32) -> std::io::Result<()> {
    let pid = pid.to_string();
    if cfg!(windows) {
        Command::new("taskkill").args(["/PID", &pid]).status()?;
    } else {
        Command::new("kill").args(["-TERM", &pid]).status()?;
    }
    Ok(())
}

/// End `pid` at once, without asking it to close first: a game that has no
/// window yet ignores a polite close.
pub fn kill_process(pid: u32) -> std::io::Result<()> {
    let pid = pid.to_string();
    if cfg!(windows) {
        Command::new("taskkill").args(["/F", "/PID", &pid]).status()?;
    } else {
        Command::new("kill").args(["-KILL", &pid]).status()?;
    }
    Ok(())
}

/// Ask the running games of the copy in `dir` to close, and return their
/// pids. A game that cannot be told to be that copy's is left running.
pub fn stop_game_in(dir: &Path) -> std::io::Result<Vec<u32>> {
    let games = games_in(dir);
    for g in &games {
        stop_process(g.pid)?;
    }
    Ok(games.into_iter().map(|g| g.pid).collect())
}

#[cfg(test)]
mod tests;
