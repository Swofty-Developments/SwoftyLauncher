//! Whether the Steam client is running and signed in, and starting it when
//! it is not.
//!
//! Windows publishes the client in `HKCU\Software\Valve\Steam\ActiveProcess`:
//! `pid` is the running steam.exe (0 once it exits) and `ActiveUser` the
//! signed-in account (0 at the sign-in screen). Some signed-in clients keep
//! `ActiveUser` at 0, so a sign-in that `config/loginusers.vdf` dates after
//! the client started counts once `ActiveUser` has had time to follow it.
//! On Linux the client writes its pid to `~/.steam/steam.pid` and
//! `registry.vdf`; current clients keep `ActiveProcess` out of
//! `registry.vdf`, so a sign-in shows as `config/loginusers.vdf` being
//! written after the client started. Flatpak Steam keeps the same files
//! under `~/.var/app/com.valvesoftware.Steam`, with pids from its own
//! namespace.

mod linux;
mod registry;
mod start;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::platform::steam::vdf;
use crate::platform::steam::vdf::Vdf;

pub use linux::{linux_status, processes};
pub use registry::{LocalRegistry, Registry, windows_status};
pub use start::{
    Ready, Report, Stage, StartError, Starter, Wait, ensure, linux_starter, starter, windows_starter,
};

pub const FLATPAK_APP: &str = "com.valvesoftware.Steam";
pub const ACTIVE_PROCESS_KEY: &str = r"HKCU\Software\Valve\Steam\ActiveProcess";
pub const STEAM_KEY: &str = r"HKCU\Software\Valve\Steam";
pub const READY_TIMEOUT: Duration = Duration::from_secs(90);
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// What the launch is waiting on, for the launcher's status line.
pub const STAGE_FILE: &str = "state/launch-stage.txt";

const LOGIN_SLACK: Duration = Duration::from_secs(5);
/// How long `ActiveUser` gets to follow a sign-in `loginusers.vdf` records.
const ACTIVE_USER_LAG: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    /// A Steam process runs, but the client has not registered yet.
    Starting,
    /// The client runs and reports nobody signed in.
    SigningIn,
    /// The client runs, and nothing says whether it has signed in.
    Unconfirmed,
    Ready,
}

impl State {
    pub fn usable(self) -> bool {
        matches!(self, Self::Ready | Self::Unconfirmed)
    }
}

/// What the sign-in state was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Unknown,
    /// `ActiveProcess\ActiveUser`: the signed-in account, 0 for nobody.
    ActiveUser(u32),
    /// `loginusers.vdf` was written after the client started.
    LoginFile,
    /// `loginusers.vdf` predates the client.
    NoLogin,
    /// `ActiveUser` stays 0, but `loginusers.vdf` records a sign-in since the
    /// client started.
    LoginOverActiveUser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub pid: Option<u32>,
    pub signal: Signal,
}

impl Status {
    pub const STOPPED: Self = Self {
        state: State::Stopped,
        pid: None,
        signal: Signal::Unknown,
    };

    fn new(state: State, pid: u32, signal: Signal) -> Self {
        Self {
            state,
            pid: Some(pid),
            signal,
        }
    }

    pub fn headline(&self) -> &'static str {
        match self.state {
            State::Stopped => "not running",
            State::Starting => "starting",
            State::SigningIn | State::Unconfirmed => "waiting for sign-in",
            State::Ready => "ready",
        }
    }

    /// The pid and the sign-in evidence, e.g. `pid 4242, ActiveUser 0`.
    pub fn detail(&self) -> String {
        let signal = match (self.state, self.signal) {
            (State::Stopped, _) => None,
            (State::Starting, _) => Some("the client hasn't registered yet".to_owned()),
            (_, Signal::ActiveUser(user)) => Some(format!("ActiveUser {user}")),
            (_, Signal::LoginFile) => Some("loginusers.vdf written since it started".to_owned()),
            (_, Signal::NoLogin) => Some("no sign-in in loginusers.vdf since it started".to_owned()),
            (_, Signal::LoginOverActiveUser) => {
                Some("ActiveUser 0, but loginusers.vdf records a sign-in since it started".to_owned())
            }
            (_, Signal::Unknown) => Some("no sign-in signal".to_owned()),
        };
        self.pid
            .map(|p| format!("pid {p}"))
            .into_iter()
            .chain(signal)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.detail() {
            d if d.is_empty() => f.write_str(self.headline()),
            d => write!(f, "{} ({d})", self.headline()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    /// The pid inside the process's own namespace (Flatpak's sandbox).
    pub ns_pid: u32,
    pub name: String,
    /// The first command-line argument.
    pub exe: String,
    pub started: Option<SystemTime>,
}

/// Whether `login` came after the client `started` and long enough ago for
/// `ActiveUser` to have followed it.
fn signed_in_since(login: Option<SystemTime>, started: Option<SystemTime>, now: SystemTime) -> bool {
    let (Some(login), Some(started)) = (login, started) else {
        return false;
    };
    login + LOGIN_SLACK >= started && now.duration_since(login).is_ok_and(|d| d >= ACTIVE_USER_LAG)
}

fn read_vdf(path: &Path) -> Option<Vdf> {
    vdf::parse(&std::fs::read_to_string(path).ok()?)
}

/// The newest `Timestamp` in the `loginusers.vdf` under `root`.
fn login_stamp(root: &Path) -> Option<SystemTime> {
    read_vdf(&root.join("config/loginusers.vdf"))
        .and_then(|v| {
            v.get("users")?
                .entries()
                .filter_map(|(_, u)| u.get("Timestamp")?.str()?.trim().parse::<u64>().ok())
                .max()
        })
        .map(|s| UNIX_EPOCH + Duration::from_secs(s))
}

/// When an account last signed in to the client under `root`.
fn last_login(root: &Path) -> Option<SystemTime> {
    let written = std::fs::metadata(root.join("config/loginusers.vdf"))
        .and_then(|m| m.modified())
        .ok();
    written.max(login_stamp(root))
}

#[cfg(windows)]
pub fn status() -> Status {
    let pids: Vec<u32> = crate::platform::process::named("steam.exe")
        .iter()
        .map(|p| p.pid)
        .collect();
    windows_status(
        &LocalRegistry,
        &pids,
        &crate::platform::process::started,
        &crate::platform::steam::steam_roots(),
    )
}

#[cfg(not(windows))]
pub fn status() -> Status {
    match std::env::var_os("HOME") {
        Some(home) => linux_status(Path::new(&home), &processes()),
        None => Status::STOPPED,
    }
}

/// True inside a session Steam itself runs (Steam Deck Game Mode, Big
/// Picture's gamescope session), where the client is always up.
pub fn steam_session() -> bool {
    std::env::var_os("SteamGamepadUI").is_some_and(|v| !v.is_empty() && v != "0")
        || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.eq_ignore_ascii_case("gamescope"))
}
