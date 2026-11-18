//! What the session looks at once version.dll did not attach to a running
//! game: the game folder's copy against the one the launcher placed, which
//! F1_25.exe runs and which version.dll it holds, and on Windows the DLL
//! policies on the game process and the Exploit protection settings that
//! turn on "prefer System32 DLLs", KnownDLLs, Smart App Control, the Code
//! Integrity and Defender events since the game started, and the antivirus
//! registered with Windows. Under Proton it is the prefix's override and the
//! version.dll Wine mapped. Every check is read-only, bounded in time, and a
//! check that fails says why and the rest go on. A loader.log of this launch
//! that holds the loader's attach line overrules the miss: the loader runs
//! and waits for the game window.

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::GAME_EXE;
use crate::diagnostics::quarantine::PreferSource;
use crate::install::{PROXY, has_override};
use crate::platform::process::GameProcess;
use crate::platform::run_as_admin::AdminLayer;
use crate::session::{LOADER_LOG, written_since};

mod attach;
mod cause;
mod events;
mod lines;
mod modules;
mod paths;
mod policy;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as platform;

#[cfg(test)]
mod tests;

pub use self::attach::{attached, created_pid};
use self::events::attr;
pub use self::events::events;
use self::modules::{SECURITY_MODULES, hooks};
pub use self::modules::{antivirus_name, mapped_modules, modules};
use self::paths::{file_name, in_folder, norm, proxy, tidy, under};
use self::policy::EXPLOIT_PROTECTION;
pub use self::policy::{policy_prefers, policy_source, prefers_system32, xml_text};

pub type Probe<T> = std::result::Result<T, String>;

/// Each Windows tool gets this long, and they run side by side.
#[cfg_attr(not(windows), allow(dead_code))]
const TOOL_WAIT: Duration = Duration::from_secs(2);
/// Events from a little before the game started count too.
#[cfg_attr(not(windows), allow(dead_code))]
const EVENT_MARGIN: Duration = Duration::from_secs(120);
const MAX_GAMES: usize = 3;
const MAX_OTHERS: usize = 24;
const MAX_EVENTS: usize = 6;

pub const CI_LOG: &str = "Microsoft-Windows-CodeIntegrity/Operational";
pub const DEFENDER_LOG: &str = "Microsoft-Windows-Windows Defender/Operational";
/// Signing level not met (and its audit), App Control policy block (and audit).
#[cfg_attr(not(windows), allow(dead_code))]
const CI_IDS: &[u32] = &[3033, 3034, 3076, 3077];
/// Threat detected and acted on, attack surface reduction block and audit,
/// Controlled folder access block and audit.
#[cfg_attr(not(windows), allow(dead_code))]
const DEFENDER_IDS: &[u32] = &[1116, 1117, 1121, 1122, 1123, 1124];
const SIGNING_LEVEL: u32 = 3033;
const POLICY_BLOCK: u32 = 3077;
/// The events that stop a file loading, as opposed to auditing it.
const BLOCKS: &[u32] = &[3033, 3077, 1116, 1117, 1121];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Findings {
    /// The game folder's version.dll.
    pub proxy: Proxy,
    /// The size of the game folder's F1_25.exe.
    pub exe_size: Option<u64>,
    /// The pid the offline play stub created, from its loader.log line.
    pub created: Option<u32>,
    pub games: Vec<Seen>,
    /// loader.prev.log was written after the game started: the loader ran
    /// and moved the log aside.
    pub rotated: bool,
    /// The loader's attach line in this launch's loader.log: the loader
    /// runs in the game, whatever the session's watch made of the log.
    pub attached: Option<Attached>,
    pub search: Option<Probe<Search>>,
    /// The value names under F1_25.exe's Image File Execution Options key,
    /// where Exploit protection keeps its program settings.
    pub exec_options: Option<Probe<Vec<String>>>,
    /// What turns on "prefer System32 DLLs" for F1_25.exe: its Image File
    /// Execution Options and their UseFilter subkeys for the game folder's
    /// F1_25.exe, Exploit protection's system settings, an Exploit
    /// protection policy.
    pub prefer_from: Option<Probe<Vec<PreferSource>>>,
    /// The compatibility settings on the game folder's F1_25.exe, as
    /// `<layers> (for this user|for all users)`.
    pub layers: Option<Probe<Vec<String>>>,
    /// The compatibility settings on the programs that start the game
    /// (`compat::starter_layers`).
    pub starter_layers: Option<Vec<AdminLayer>>,
    /// The EA app processes running, and the compatibility shims in each.
    pub ea_app: Vec<EaApp>,
    /// VerifiedAndReputablePolicyState: 0 off, 1 on, 2 evaluation.
    pub smart_app_control: Option<Probe<Option<u32>>>,
    pub logs: Vec<EventLog>,
    pub antivirus: Option<Probe<Vec<String>>>,
    /// Proton: the prefix's user.reg holds the version.dll override.
    pub wine_override: Option<bool>,
}

/// The loader's last attach line in a loader.log, and what follows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attached {
    pub pid: u32,
    /// The clock time on the attach line, in the PC's time zone.
    pub at: Option<String>,
    /// The loader logged the window stage, where the addons are loaded.
    pub window: bool,
    /// From the attach line to the window stage's line.
    pub window_after: Option<Duration>,
    /// The log's last line, and how long after the attach line it came.
    pub last: Option<String>,
    pub last_after: Option<Duration>,
    /// How long ago the game was started.
    pub ran: Option<Duration>,
    /// What ties the log to this launch.
    pub proof: Proof,
}

/// Why a loader.log is this launch's and not one an earlier launch left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Proof {
    /// The pid on the attach line is a running F1_25.exe.
    #[default]
    Running,
    /// loader.prev.log and loader.log were both written after the game
    /// started: the loader moved the old log aside and began this one.
    Rotated,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Proxy {
    #[default]
    Missing,
    /// `same`: whether it holds the bytes the launcher placed, when there is
    /// a record of them; `zone`: its Zone.Identifier mark.
    Present {
        size: u64,
        same: Option<bool>,
        zone: Option<u32>,
    },
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub pid: u32,
    pub image: Option<String>,
    pub image_size: Option<u64>,
    /// The image is the game folder's F1_25.exe.
    pub ours: bool,
    pub modules: Probe<Modules>,
    pub mitigation: Option<Probe<Mitigation>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EaApp {
    pub pid: u32,
    pub image: Option<String>,
    pub shims: Probe<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Modules {
    pub count: usize,
    /// Every version.dll the process holds, by full path.
    pub version: Vec<String>,
    /// The file names of modules from outside the game and Windows folders.
    pub others: Vec<String>,
    /// The compatibility shim DLLs Windows loaded, which a compatibility
    /// setting or a shim database brings in.
    pub shims: Vec<String>,
}

/// The process's DLL load policies (GetProcessMitigationPolicy).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mitigation {
    pub microsoft_signed: bool,
    pub store_signed: bool,
    pub prefer_system32: bool,
    pub no_remote: bool,
    pub no_low_label: bool,
}

impl Mitigation {
    pub fn names(&self) -> Vec<&'static str> {
        [
            (self.microsoft_signed, "Microsoft-signed DLLs only"),
            (self.store_signed, "Store-signed DLLs only"),
            (self.prefer_system32, "prefer System32 DLLs"),
            (self.no_remote, "no DLLs from network shares"),
            (self.no_low_label, "no low-integrity DLLs"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Search {
    pub known_dll: bool,
    pub safe_search: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventLog {
    pub name: &'static str,
    pub read: Probe<Vec<Event>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Event {
    pub id: u32,
    /// The UTC clock time it was logged.
    pub at: Option<String>,
    pub file: Option<String>,
    pub process: Option<String>,
    pub policy: Option<String>,
    pub threat: Option<String>,
    /// It names version.dll.
    pub proxy: bool,
}

impl Event {
    pub fn blocks(&self) -> bool {
        BLOCKS.contains(&self.id)
    }

    fn what(&self) -> &'static str {
        match self.id {
            3033 => "blocked: below the signing level the process requires",
            3034 => "audit: would be blocked for its signing level",
            3076 => "audit: an App Control policy would block it",
            3077 => "blocked by an App Control policy",
            1116 => "detected as a threat",
            1117 => "acted on as a threat",
            1121 => "blocked by an attack surface reduction rule",
            1122 => "audit: an attack surface reduction rule would block it",
            1123 => "blocked by Controlled folder access",
            1124 => "audit: Controlled folder access would block it",
            _ => "logged",
        }
    }
}

/// Look at what could keep version.dll out of the running game. `placed` is
/// the SHA-256 of the version.dll the launcher placed, `since` when the game
/// was started, `user_reg` the Proton prefix's user.reg.
pub fn collect(game: &Path, placed: Option<&str>, since: SystemTime, user_reg: Option<&Path>) -> Findings {
    let log = game.join(LOADER_LOG);
    let stub = std::fs::read(&log)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut f = Findings {
        proxy: proxy(&game.join(PROXY), placed),
        exe_size: std::fs::metadata(game.join(GAME_EXE)).ok().map(|m| m.len()),
        created: created_pid(&stub),
        rotated: written_since(&log.with_file_name("loader.prev.log"), since),
        wine_override: user_reg
            .filter(|_| !cfg!(windows))
            .map(|r| std::fs::read_to_string(r).is_ok_and(|t| has_override(&t))),
        ..Findings::default()
    };
    let running = crate::platform::process::game_processes();
    platform::fill(&mut f, game, &running, since);
    let pids: Vec<u32> = running.iter().map(|g| g.pid).collect();
    let fresh = f.rotated && written_since(&log, since);
    f.attached = attached(&stub, &pids, fresh).map(|a| Attached {
        ran: since.elapsed().ok(),
        ..a
    });
    f
}

fn seen_base(g: &GameProcess, game: &Path, modules: Probe<Modules>) -> Seen {
    let image = g.origin.image.clone();
    let image_size = image
        .as_deref()
        .and_then(|p| std::fs::metadata(p.strip_prefix(r"\\?\").unwrap_or(p)).ok())
        .map(|m| m.len());
    Seen {
        pid: g.pid,
        image,
        image_size,
        ours: g.is_in(game),
        modules,
        mitigation: None,
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub fn fill(f: &mut Findings, game: &Path, running: &[GameProcess], _: SystemTime) {
        f.games = running
            .iter()
            .take(MAX_GAMES)
            .map(|g| {
                let maps = std::fs::read_to_string(format!("/proc/{}/maps", g.pid))
                    .map(|m| mapped_modules(&m, game))
                    .map_err(|e| e.to_string());
                seen_base(g, game, maps)
            })
            .collect();
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::*;

    pub fn fill(_: &mut Findings, _: &Path, _: &[GameProcess], _: SystemTime) {}
}
