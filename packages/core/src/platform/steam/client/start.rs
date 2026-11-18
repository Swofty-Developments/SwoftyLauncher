//! Starting the client and waiting until it is signed in.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::registry::{Registry, steam_key_values};
use super::{FLATPAK_APP, POLL_INTERVAL, READY_TIMEOUT, State, Status};

pub(super) const EXIT_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub enum Starter {
    Exe(PathBuf),
    Flatpak(PathBuf),
    /// Windows without a known steam.exe: the `steam://` handler.
    Url,
}

impl Starter {
    /// The client with `args` (`-applaunch`, `-silent`); None for [`Starter::Url`].
    pub fn command<S: AsRef<OsStr>>(&self, args: &[S]) -> Option<Command> {
        let mut c = match self {
            Self::Exe(exe) => Command::new(exe),
            Self::Flatpak(flatpak) => {
                let mut c = Command::new(flatpak);
                c.args(["run", FLATPAK_APP]);
                c
            }
            Self::Url => return None,
        };
        c.args(args);
        Some(c)
    }

    /// Start the client in the background, detached from this process. On
    /// Windows, a registered steam.exe that will not start falls back to the
    /// `steam://` handler.
    pub fn start(&self) -> std::io::Result<Child> {
        match self.spawn_silent() {
            Err(e) if cfg!(windows) && matches!(self, Self::Exe(_)) => {
                Self::Url.spawn_silent().map_err(|_| e)
            }
            spawned => spawned,
        }
    }

    fn spawn_silent(&self) -> std::io::Result<Child> {
        let mut c = self.command(&["-silent"]).unwrap_or_else(|| {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", "", "steam://open/main"]);
            c
        });
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut c, 0);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            c.creation_flags(match self {
                Self::Url => CREATE_NO_WINDOW,
                _ => DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
            });
        }
        c.spawn()
    }
}

impl std::fmt::Display for Starter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exe(exe) => write!(f, "{} -silent", exe.display()),
            Self::Flatpak(_) => write!(f, "flatpak run {FLATPAK_APP} -silent"),
            Self::Url => f.write_str("steam://open/main"),
        }
    }
}

/// The client to start on Linux: `steam` on `path`, or Flatpak Steam, the
/// Flatpak one first when the game's Steam install is Flatpak's.
pub fn linux_starter(path: Option<&OsStr>, home: &Path, install: &[&Path]) -> Option<Starter> {
    let find = |bin: &str| {
        path.and_then(|p| {
            std::env::split_paths(p)
                .map(|d| d.join(bin))
                .find(|f| f.is_file())
        })
    };
    let app = home.join(".var/app").join(FLATPAK_APP);
    let in_flatpak = install.iter().any(|p| p.starts_with(&app));
    let flatpak = find("flatpak")
        .filter(|_| in_flatpak || app.is_dir())
        .map(Starter::Flatpak);
    let native = find("steam").map(Starter::Exe);
    if in_flatpak {
        flatpak.or(native)
    } else {
        native.or(flatpak)
    }
}

/// steam.exe from `SteamExe` or `SteamPath`, else a Steam root's, else the
/// `steam://` handler.
pub fn windows_starter(reg: &dyn Registry, roots: &[PathBuf]) -> Starter {
    let values = steam_key_values(reg);
    values
        .path("SteamExe")
        .or_else(|| values.path("SteamPath").map(|p| p.join("steam.exe")))
        .or_else(|| roots.iter().map(|r| r.join("steam.exe")).find(|e| e.is_file()))
        .map_or(Starter::Url, Starter::Exe)
}

#[cfg(windows)]
pub fn starter(_install: &[&Path]) -> Option<Starter> {
    Some(windows_starter(
        &super::LocalRegistry,
        &crate::platform::steam::steam_roots(),
    ))
}

#[cfg(not(windows))]
pub fn starter(install: &[&Path]) -> Option<Starter> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    linux_starter(std::env::var_os("PATH").as_deref(), &home, install)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Starting,
    SigningIn,
}

impl Stage {
    pub fn text(self) -> &'static str {
        match self {
            Self::Starting => "Starting Steam…",
            Self::SigningIn => "Waiting for Steam to sign in…",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    /// Steam as the launch found it.
    Detected(Status),
    /// Steam changed while the launch waited, this long into the wait.
    Changed(Status, Duration),
    /// For the launcher's status line; each stage once.
    Stage(Stage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ready {
    /// Steam was already up.
    Running,
    /// Steam was started, or finished starting, and signed in.
    Started,
    /// The client is up, but it never said it signed in.
    Unconfirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    #[error("Steam didn't start within {0} s; open Steam and press Play again")]
    Timeout(u64),
    #[error("Steam is open but not signed in; sign in to Steam and press Play again")]
    SignedOut,
    #[error("could not start Steam ({0}); open Steam and press Play again")]
    Spawn(String),
}

#[derive(Debug, Clone, Copy)]
pub struct Wait {
    pub timeout: Duration,
    pub poll: Duration,
}

impl Default for Wait {
    fn default() -> Self {
        Self {
            timeout: READY_TIMEOUT,
            poll: POLL_INTERVAL,
        }
    }
}

pub(super) struct Reaped(Option<Child>);

impl Drop for Reaped {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            std::thread::spawn(move || child.wait().ok());
        }
    }
}

/// Bring Steam up: start it when no Steam process runs, then wait until it
/// is signed in, reporting what it sees as it changes.
pub fn ensure(
    mut detect: impl FnMut() -> Status,
    start: impl FnOnce() -> std::io::Result<Child>,
    wait: Wait,
    mut report: impl FnMut(Report),
) -> Result<Ready, StartError> {
    let mut status = detect();
    report(Report::Detected(status));
    if status.state.usable() {
        return Ok(Ready::Running);
    }
    let mut shown = None;
    let mut emit = |r: Report| {
        if let Report::Stage(stage) = r {
            if shown == Some(stage) {
                return;
            }
            shown = Some(stage);
        }
        report(r);
    };
    let mut child = Reaped(None);
    if status.state == State::Stopped {
        emit(Report::Stage(Stage::Starting));
        child.0 = Some(start().map_err(|e| StartError::Spawn(e.to_string()))?);
    }
    let since = Instant::now();
    let mut failed: Option<(Instant, ExitStatus)> = None;
    loop {
        let state = status.state;
        match state {
            State::Ready => return Ok(Ready::Started),
            State::Stopped | State::Starting => emit(Report::Stage(Stage::Starting)),
            State::SigningIn | State::Unconfirmed => emit(Report::Stage(Stage::SigningIn)),
        }
        if let Some(c) = child.0.as_mut()
            && let Ok(Some(exit)) = c.try_wait()
        {
            child.0 = None;
            if !exit.success() {
                failed = Some((Instant::now(), exit));
            }
        }
        match failed {
            Some((at, exit)) if state == State::Stopped && at.elapsed() >= EXIT_GRACE => {
                return Err(StartError::Spawn(format!("Steam exited with {exit}")));
            }
            Some(_) if state != State::Stopped => failed = None,
            _ => {}
        }
        if since.elapsed() >= wait.timeout {
            break;
        }
        std::thread::sleep(wait.poll);
        let now = detect();
        if now != status {
            emit(Report::Changed(now, since.elapsed()));
        }
        status = now;
    }
    match status.state {
        State::Unconfirmed => Ok(Ready::Unconfirmed),
        State::SigningIn => Err(StartError::SignedOut),
        _ => Err(StartError::Timeout(wait.timeout.as_secs())),
    }
}
