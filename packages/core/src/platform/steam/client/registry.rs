//! The client as Windows publishes it: the `ActiveProcess` key and the sign-in `loginusers.vdf` dates.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::platform::run_as_admin::{parse_reg_query, reg_dword};

use super::{ACTIVE_PROCESS_KEY, STEAM_KEY, Signal, State, Status, login_stamp, signed_in_since};

pub trait Registry {
    /// What `reg query <key>` prints, or None when the key cannot be read.
    fn query(&self, key: &str) -> Option<String>;
}

/// This computer's registry, read in process ([`crate::platform::registry`]).
pub struct LocalRegistry;

impl Registry for LocalRegistry {
    fn query(&self, key: &str) -> Option<String> {
        crate::platform::registry::reg_query(&[key])
    }
}

/// Steam on Windows. `started` dates a steam.exe, and `roots` are Steam
/// folders to look for `loginusers.vdf` in besides the registered one.
pub fn windows_status(
    reg: &dyn Registry,
    steam_pids: &[u32],
    started: &dyn Fn(u32) -> Option<SystemTime>,
    roots: &[PathBuf],
) -> Status {
    let active = reg.query(ACTIVE_PROCESS_KEY).unwrap_or_default();
    match reg_dword(&active, "pid").filter(|p| *p != 0) {
        Some(pid) if steam_pids.contains(&pid) => match reg_dword(&active, "ActiveUser") {
            Some(0) if signed_in_since(windows_login(reg, roots), started(pid), SystemTime::now()) => {
                Status::new(State::Ready, pid, Signal::LoginOverActiveUser)
            }
            Some(0) => Status::new(State::SigningIn, pid, Signal::ActiveUser(0)),
            Some(user) => Status::new(State::Ready, pid, Signal::ActiveUser(user)),
            None => Status::new(State::Unconfirmed, pid, Signal::Unknown),
        },
        _ => match steam_pids.first() {
            Some(&pid) => Status::new(State::Starting, pid, Signal::Unknown),
            None => Status::STOPPED,
        },
    }
}

/// The newest sign-in `loginusers.vdf` records under the registered Steam
/// folder or `roots`.
pub(super) fn windows_login(reg: &dyn Registry, roots: &[PathBuf]) -> Option<SystemTime> {
    let registered = steam_key_values(reg);
    registered
        .path("SteamPath")
        .into_iter()
        .chain(
            registered
                .path("SteamExe")
                .and_then(|e| Some(e.parent()?.to_path_buf())),
        )
        .chain(roots.iter().cloned())
        .filter_map(|r| login_stamp(&r))
        .max()
}

pub(super) struct RegValues(Vec<(String, String)>);

impl RegValues {
    pub(super) fn path(&self, name: &str) -> Option<PathBuf> {
        self.0
            .iter()
            .find(|(n, d)| n.eq_ignore_ascii_case(name) && !d.trim().is_empty())
            .map(|(_, d)| PathBuf::from(d.trim()))
    }
}

pub(super) fn steam_key_values(reg: &dyn Registry) -> RegValues {
    RegValues(
        reg.query(STEAM_KEY)
            .map(|l| parse_reg_query(&l))
            .unwrap_or_default(),
    )
}
