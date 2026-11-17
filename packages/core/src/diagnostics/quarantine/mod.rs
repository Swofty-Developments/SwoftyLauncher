//! Loader files an antivirus took away. Windows Defender and other scanners
//! delete, quarantine or block version.dll and the core DLLs, often a moment
//! after they are written, and F1 25 then runs with no mods at all. The
//! session checks the placed DLLs before the game starts and watches the
//! game folder's loader.log after it starts; what it finds is recorded here
//! for the launcher to show and for Send Logs. Missing files before the game
//! starts refuse the launch: the session rolls back and the game never runs.

mod attach;
mod cause;
mod missing;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::install::PROXY;
use crate::{Result, io_err, read_json, write_json};

pub use attach::{
    ATTACH_WINDOW, ATTACHED, Attach, AttachWatch, PROBE_AFTER, RECHECK_AFTER, attached, refused, refused_text,
};
pub use cause::{Carrier, Cause, EXEC_OPTIONS_KEY, PreferSource};
pub use missing::{LOADER_FOLDER, damaged_install, missing, placed_dlls};

pub const ERROR_VIRUS_INFECTED: i32 = 225;
pub const ERROR_VIRUS_DELETED: i32 = 226;
/// The exit code of a session that refused to start the game over missing
/// loader files: clear of the 0/1 the CLI uses and of the game's own codes.
pub const REFUSED_EXIT: u8 = 225;

pub fn is_virus_code(code: i32) -> bool {
    matches!(code, ERROR_VIRUS_INFECTED | ERROR_VIRUS_DELETED)
}

/// Windows refuses to copy or open a file it detected as a threat with these
/// two codes; elsewhere the numbers mean nothing.
pub fn is_virus_error(e: &std::io::Error) -> bool {
    cfg!(windows) && e.raw_os_error().is_some_and(is_virus_code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    BeforeLaunch,
    InGame,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub at: f64,
    pub stage: Stage,
    /// Paths relative to the game folder, or to the launcher folder for a
    /// loader file missing from the install itself.
    pub files: Vec<String>,
    /// In game: version.dll was gone from the game folder when checked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
    pub game: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<PathBuf>,
    /// In game: what the session found keeping version.dll out of the game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<Box<Cause>>,
}

impl Report {
    pub fn new(stage: Stage, files: Vec<String>, game: &Path) -> Self {
        Self {
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64()),
            stage,
            files,
            removed: false,
            game: game.to_path_buf(),
            launcher: launcher_dir(),
            cause: None,
        }
    }

    pub fn path(data: &Path) -> PathBuf {
        data.join("state/mods-blocked.json")
    }

    pub fn write(&self, data: &Path) -> Result<()> {
        write_json(&Self::path(data), self)
    }

    pub fn read(data: &Path) -> Option<Self> {
        read_json(&Self::path(data)).ok()
    }

    pub fn clear(data: &Path) -> Result<()> {
        let path = Self::path(data);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io_err(&path)(e)),
            _ => Ok(()),
        }
    }

    fn no_loader_folder(&self) -> bool {
        self.files.iter().any(|f| f == LOADER_FOLDER)
    }

    pub fn title(&self) -> &'static str {
        match self.stage {
            Stage::BeforeLaunch if self.no_loader_folder() => "Swofty Launcher's loader folder is missing",
            Stage::BeforeLaunch => "Your antivirus removed mod files",
            Stage::InGame => "Mods did not load",
        }
    }

    pub fn text(&self) -> String {
        if self.stage == Stage::BeforeLaunch && self.no_loader_folder() {
            let launcher = self
                .launcher
                .as_ref()
                .map(|l| format!(" ({})", l.display()))
                .unwrap_or_default();
            return format!(
                "F1 25 was not started: your Swofty Launcher install has no loader folder, the part that puts the \
                 mods into the game. To fix it, reinstall Swofty Launcher with the installer from f1.swofty.net; \
                 your addons and settings are kept. If the folder goes missing again, your antivirus removed it: \
                 restore it from the antivirus's quarantine and add the launcher folder{launcher} to its \
                 exclusions."
            );
        }
        let names = names(&self.files);
        let one = self.files.len() <= 1;
        let it = if one { "it" } else { "them" };
        let cause = self
            .cause
            .as_deref()
            .filter(|_| self.stage == Stage::InGame && !self.removed);
        let what = match (self.stage, cause) {
            (Stage::BeforeLaunch, _) => {
                format!("F1 25 was not started: an antivirus most likely deleted or quarantined {names}.")
            }
            (Stage::InGame, _) if self.removed => format!(
                "F1 25 is running without mods: {names} {} removed from the game folder. An antivirus most likely deleted or quarantined {it}.",
                if one { "was" } else { "were" }
            ),
            (Stage::InGame, Some(c)) => format!("F1 25 is running without mods: {}.", c.what()),
            (Stage::InGame, None) => {
                format!("F1 25 is running without mods. An antivirus most likely blocked {names}.")
            }
        };
        let fix = cause
            .and_then(|c| c.fix(&self.game))
            .unwrap_or_else(|| self.fix());
        format!("{what} {fix}")
    }

    fn fix(&self) -> String {
        let it = if self.files.len() <= 1 { "it" } else { "them" };
        let folders = match &self.launcher {
            Some(l) => format!(
                "the game folder ({}) and the launcher folder ({})",
                self.game.display(),
                l.display()
            ),
            None => format!(
                "the game folder ({}) and the launcher folder",
                self.game.display()
            ),
        };
        format!(
            "To fix it, {}restore {it} in Windows Security > Virus & threat protection > \
             Protection history, add exclusions for {folders}, then {}.",
            match self.stage {
                Stage::BeforeLaunch => "",
                Stage::InGame => "close F1 25, ",
            },
            match self.stage {
                Stage::BeforeLaunch => "press Play again",
                Stage::InGame => "launch again",
            }
        )
    }
}

fn names(files: &[String]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for f in files {
        let name = f.rsplit(['/', '\\']).next().unwrap_or(f);
        if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            names.push(name);
        }
    }
    match names.as_slice() {
        [] => PROXY.to_owned(),
        [a] => (*a).to_owned(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] if rest.len() < 4 => format!("{} and {last}", rest.join(", ")),
        _ => format!("{} and {} more", names[..3].join(", "), names.len() - 3),
    }
}

/// The folder the launcher is installed in, for the exclusion.
fn launcher_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.to_path_buf())
}

/// A failed copy into the game folder; one Windows refused as a threat says
/// so and what to do about it.
pub(crate) fn copy_error(rel: &str, path: &Path, e: std::io::Error) -> crate::Error {
    if !is_virus_error(&e) {
        return io_err(path)(e);
    }
    let game = path
        .ancestors()
        .nth(rel.split(['/', '\\']).count())
        .unwrap_or(path);
    crate::Error::Other(format!(
        "Windows blocked {} as a threat ({e}). {}",
        names(&[rel.to_owned()]),
        Report::new(Stage::BeforeLaunch, vec![rel.to_owned()], game).fix()
    ))
}
