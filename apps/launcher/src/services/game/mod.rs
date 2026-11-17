use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use swofty_core::install::Source;
use swofty_core::platform::ea_app;
use swofty_core::platform::process::GameProcess;

mod crash;
mod discovery;
mod import_retry;
mod probe;
mod restore;
mod run;
#[cfg(test)]
mod tests;
mod vault_watch;
pub(crate) use crash::crash_summary;
pub(crate) use import_retry::import_now;
pub(crate) use probe::{Probe, blocked_at, probe};
pub(crate) use restore::{recover_stale, restore};
pub(crate) use run::{Launch, cause_first, cli_path};
pub(crate) use vault_watch::{banner_stale, vault_at_start};

#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    pub dir: PathBuf,
    pub build_id: Option<u64>,
    pub exe_version: Option<String>,
    pub mode: String,
    pub via_steam: bool,
    pub manual: bool,
    pub source: Source,
    pub moved: Option<ea_app::Moved>,
}

impl Install {
    pub fn version(&self) -> Option<String> {
        self.exe_version
            .clone()
            .or_else(|| self.build_id.map(|b| b.to_string()))
    }
}

/// Whose a running F1 25 is: the active copy's, another listed copy's, or
/// none the launcher can tell.
#[derive(Debug, Clone, PartialEq)]
pub enum Owner {
    Active,
    Other(Install),
    Unknown,
}

pub fn owner(g: &GameProcess, active: Option<&Install>, copies: &[Install]) -> Owner {
    if active.is_some_and(|a| g.is_in(&a.dir)) {
        return Owner::Active;
    }
    copies
        .iter()
        .find(|c| g.is_in(&c.dir))
        .cloned()
        .map_or(Owner::Unknown, Owner::Other)
}

/// Every copy found, and the one Play uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Copies {
    pub all: Vec<Install>,
    pub active: Option<Install>,
    pub missing: Vec<PathBuf>,
}

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
