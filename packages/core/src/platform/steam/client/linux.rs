//! The client on Linux and in Flatpak: its pid file, `registry.vdf` and the running processes.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{FLATPAK_APP, LOGIN_SLACK, Proc, Signal, State, Status, last_login, read_vdf};

pub(super) struct Install {
    dot: PathBuf,
    roots: Vec<PathBuf>,
    sandboxed: bool,
}

pub(super) fn installs(home: &Path) -> [Install; 2] {
    let flatpak = home.join(".var/app").join(FLATPAK_APP);
    [
        Install {
            dot: home.join(".steam"),
            roots: vec![
                home.join(".steam/steam"),
                home.join(".steam/root"),
                home.join(".local/share/Steam"),
            ],
            sandboxed: false,
        },
        Install {
            dot: flatpak.join(".steam"),
            roots: vec![flatpak.join(".local/share/Steam"), flatpak.join("data/Steam")],
            sandboxed: true,
        },
    ]
}

pub(super) fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub(super) fn login_state(
    active_user: Option<u32>,
    login: Option<SystemTime>,
    started: Option<SystemTime>,
) -> (State, Signal) {
    match (active_user, login, started) {
        (Some(0), ..) => (State::SigningIn, Signal::ActiveUser(0)),
        (Some(user), ..) => (State::Ready, Signal::ActiveUser(user)),
        (None, _, None) => (State::Ready, Signal::Unknown),
        (None, Some(login), Some(started)) if login + LOGIN_SLACK >= started => {
            (State::Ready, Signal::LoginFile)
        }
        _ => (State::Unconfirmed, Signal::NoLogin),
    }
}

pub(super) fn is_client_binary(exe: &str) -> bool {
    exe.replace('\\', "/").ends_with("ubuntu12_32/steam")
}

pub fn linux_status(home: &Path, procs: &[Proc]) -> Status {
    let installs = installs(home);
    for i in &installs {
        let reg = read_vdf(&i.dot.join("registry.vdf"));
        let value = |keys: &[&str]| reg.as_ref()?.path(keys)?.str()?.trim().parse::<u32>().ok();
        let active = |name| {
            value(&[
                "Registry",
                "HKCU",
                "Software",
                "Valve",
                "Steam",
                "ActiveProcess",
                name,
            ])
        };
        let (active_pid, active_user) = (active("pid"), active("ActiveUser"));
        let pids: Vec<u32> = [
            read_pid(&i.dot.join("steam.pid")),
            read_pid(&i.dot.join("steam/steam.pid")),
            value(&["Registry", "HKLM", "Software", "Valve", "Steam", "SteamPID"]),
            active_pid,
        ]
        .into_iter()
        .flatten()
        .filter(|p| *p != 0)
        .collect();
        let is = |p: &Proc, pid: u32| p.pid == pid || (i.sandboxed && p.ns_pid == pid);
        let Some(client) = procs
            .iter()
            .find(|p| p.name == "steam" && pids.iter().any(|&pid| is(p, pid)))
        else {
            continue;
        };
        let user = active_user.filter(|_| active_pid.is_some_and(|pid| is(client, pid)));
        let login = i.roots.iter().filter_map(|r| last_login(r)).max();
        let (state, signal) = login_state(user, login, client.started);
        return Status::new(state, client.pid, signal);
    }
    if let Some(client) = procs
        .iter()
        .find(|p| p.name == "steam" && is_client_binary(&p.exe))
    {
        let login = installs
            .iter()
            .flat_map(|i| &i.roots)
            .filter_map(|r| last_login(r))
            .max();
        let (state, signal) = login_state(None, login, client.started);
        return Status::new(state, client.pid, signal);
    }
    match procs.iter().find(|p| p.name == "steam") {
        Some(p) => Status::new(State::Starting, p.pid, Signal::Unknown),
        None => Status::STOPPED,
    }
}

/// This user's processes named `steam`.
#[cfg(target_os = "linux")]
pub fn processes() -> Vec<Proc> {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let me = std::fs::metadata("/proc/self").map(|m| m.uid()).ok();
    entries
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let dir = e.path();
            if e.metadata().map(|m| m.uid()).ok() != me {
                return None;
            }
            let comm = std::fs::read_to_string(dir.join("comm")).ok()?;
            let name = comm.trim_end();
            if name != "steam" {
                return None;
            }
            let stat = std::fs::read_to_string(dir.join("stat")).ok()?;
            let state = stat.rsplit_once(')')?.1.split_whitespace().next()?;
            if matches!(state, "Z" | "X") {
                return None;
            }
            let cmdline = std::fs::read(dir.join("cmdline")).unwrap_or_default();
            let exe = cmdline.split(|b| *b == 0).next().unwrap_or_default();
            let ns_pid = std::fs::read_to_string(dir.join("status"))
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find_map(|l| l.strip_prefix("NSpid:"))?
                        .split_whitespace()
                        .last()?
                        .parse()
                        .ok()
                })
                .unwrap_or(pid);
            Some(Proc {
                pid,
                ns_pid,
                name: name.to_owned(),
                exe: String::from_utf8_lossy(exe).into_owned(),
                started: crate::platform::process::started(pid),
            })
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
pub fn processes() -> Vec<Proc> {
    Vec::new()
}
