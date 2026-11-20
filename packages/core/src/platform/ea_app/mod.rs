//! The EA app's copy of F1 25: where Windows has it installed, the link that
//! asks the EA app to launch it, and the command that hands the link over.

use std::path::{Path, PathBuf};

use crate::install::Source;
use crate::platform::ensure::EaClient;
use crate::platform::run_as_admin::parse_reg_query;
use crate::platform::{ClientNote, ClientReady, ClientStart, LaunchPlatform};

mod handoff;
mod installs;
mod link;
mod wine;

pub use handoff::{
    DESKTOP_SHELL, Handoff, NO_HANDOFF_LINUX, NO_HANDOFF_WINDOWS, desktop_opener, handoff, launcher_exe,
    no_handoff, opener, split_command, split_handler,
};
pub use installs::{
    Moved, find_installs, installs, is_install, library_dirs, moved, recorded, recorded_here,
};
pub use link::{content_ids, install_url, launch_url};
#[cfg(not(windows))]
pub use wine::prefix_server;
pub use wine::prefix_user_reg;

pub const MARKER: &str = "__Installer/installerdata.xml";
pub const PROCESS: &str = "EADesktop.exe";
pub const LAUNCHER_EXE: &str = "EALauncher.exe";

/// The game's own installerdata.xml names the first key as its install check.
pub const INSTALL_KEYS: &[&str] = swofty_addon::game::F1_25_EA_KEYS;
pub const UNINSTALL_KEYS: &[&str] = &[
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
];
pub const PROTOCOL_KEY: &str = r"HKCR\origin2\shell\open\command";
pub const DESKTOP_KEYS: &[&str] = &[
    r"HKLM\SOFTWARE\Electronic Arts\EA Desktop",
    r"HKLM\SOFTWARE\WOW6432Node\Electronic Arts\EA Desktop",
];

/// The EA app, as a [`LaunchPlatform`].
pub struct EaApp;

impl LaunchPlatform for EaApp {
    fn name(&self) -> &'static str {
        "EA app"
    }

    fn source(&self) -> Source {
        Source::EaApp
    }

    fn installed(&self, dir: &Path) -> bool {
        dir.join(MARKER).is_file()
    }

    fn installs(&self) -> Vec<PathBuf> {
        installs()
    }

    fn client_running(&self) -> bool {
        running()
    }

    fn move_hint(&self) -> &'static str {
        " with EA app > F1 25 > Manage > Move, not File Explorer"
    }

    fn ensure_client(
        &self,
        start: ClientStart<'_>,
        note: &mut dyn FnMut(ClientNote),
    ) -> Result<ClientReady, String> {
        if !self.installed(start.game) {
            return Ok(ClientReady::NotNeeded);
        }
        EaClient::for_launch(start).ensure(start.allowed, note)
    }
}

/// The install folders a `reg query` lists, by any of the value names EA
/// and Windows use for one.
pub fn install_dirs(reg_out: &str) -> Vec<PathBuf> {
    parse_reg_query(reg_out)
        .into_iter()
        .filter(|(name, _)| {
            ["Install Dir", "InstallDir", "InstallLocation"]
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n))
        })
        .map(|(_, data)| data.trim().trim_matches('"').to_owned())
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// `reg query <key> /s` output split into each subkey's values.
pub fn reg_tree(out: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("HKEY_") {
            blocks.push((line.to_owned(), String::new()));
        } else if let Some((_, values)) = blocks.last_mut() {
            values.push_str(line);
            values.push('\n');
        }
    }
    blocks
}

/// What `reg query <args>` prints for an install record or the link handler, read in process; None when
/// the key is not there.
#[cfg(windows)]
fn read_reg(args: &[&str]) -> Option<String> {
    crate::platform::registry::reg_query(args).filter(|o| !o.trim().is_empty())
}

pub fn running() -> bool {
    !crate::platform::process::named(PROCESS).is_empty()
}

#[cfg(test)]
mod tests;
