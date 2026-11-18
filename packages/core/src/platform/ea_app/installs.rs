//! Where F1 25's EA app copies are: the folders Windows and the EA app record it in, the EA app's library
//! folders, and a copy moved out of the folder the EA app recorded.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use super::read_reg;
use super::{INSTALL_KEYS, MARKER, UNINSTALL_KEYS, install_dirs, reg_tree};
use crate::GAME_EXE;
use crate::platform::run_as_admin::parse_reg_query;

pub fn is_install(dir: &Path) -> bool {
    dir.join(GAME_EXE).is_file() && dir.join(MARKER).is_file()
}

/// Every F1 25 install folder Windows records for the EA app, checked with
/// `is_install`. `reg` runs `reg query` with the given arguments.
pub fn find_installs(
    reg: impl Fn(&[&str]) -> Option<String>,
    defaults: &[PathBuf],
    is_install: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let keyed = INSTALL_KEYS
        .iter()
        .filter_map(|k| reg(&[k]))
        .flat_map(|out| install_dirs(&out));
    let uninstall = UNINSTALL_KEYS
        .iter()
        .filter_map(|k| reg(&[k, "/s"]))
        .flat_map(|out| reg_tree(&out))
        .flat_map(|(_, values)| install_dirs(&values));
    let folders = defaults.iter().flat_map(|d| {
        std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
    });
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in keyed.chain(uninstall).chain(folders) {
        if is_install(&dir) && !out.iter().any(|o| crate::game_folder::copies::same_dir(o, &dir)) {
            out.push(dir);
        }
    }
    out
}

pub(super) fn names_f1_25(values: &str) -> bool {
    parse_reg_query(values).iter().any(|(name, data)| {
        let plain: String = data
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
            .collect();
        let plain = plain
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        name.eq_ignore_ascii_case("DisplayName") && (plain == "f1 25" || plain.ends_with(" f1 25"))
    })
}

/// The folders the EA app records F1 25 in, whether or not the game is
/// still there. `reg` runs `reg query` with the given arguments.
pub fn recorded(reg: impl Fn(&[&str]) -> Option<String>) -> Vec<PathBuf> {
    let keyed = INSTALL_KEYS
        .iter()
        .filter_map(|k| reg(&[k]))
        .flat_map(|out| install_dirs(&out));
    let listed = UNINSTALL_KEYS
        .iter()
        .filter_map(|k| reg(&[k, "/s"]))
        .flat_map(|out| reg_tree(&out))
        .filter(|(_, values)| names_f1_25(values))
        .flat_map(|(_, values)| install_dirs(&values));
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in keyed.chain(listed) {
        if !out.iter().any(|o| same_folder(o, &dir)) {
            out.push(dir);
        }
    }
    out
}

#[cfg(windows)]
pub fn recorded_here() -> Vec<PathBuf> {
    recorded(read_reg)
}

#[cfg(not(windows))]
pub fn recorded_here() -> Vec<PathBuf> {
    Vec::new()
}

pub(super) fn same_folder(a: &Path, b: &Path) -> bool {
    let plain = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    crate::game_folder::copies::same_dir(a, b) || (cfg!(windows) && plain(a) == plain(b))
}

/// An EA app copy in a folder the EA app has no record of: moved by hand,
/// so the EA app's launch link starts the recorded copy, or none.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub dir: PathBuf,
    pub recorded: PathBuf,
    /// Whether the game is in the recorded folder.
    pub there: bool,
}

impl Moved {
    pub fn text(&self) -> String {
        let fix = if self.there {
            format!("Play the copy in {} instead", self.recorded.display())
        } else {
            format!("Move the folder back to {}", self.recorded.display())
        };
        format!(
            "The EA app has F1 25 in {}, so it can't start the copy in {}. {fix}. To keep F1 25 somewhere \
             else, move it with EA app > F1 25 > Manage > Move, not File Explorer",
            self.recorded.display(),
            self.dir.display()
        )
    }
}

/// `dir` against the folders the EA app records F1 25 in. Nothing recorded
/// says nothing either way, and neither does a record with a `?` or U+FFFD:
/// reg.exe writes a letter its code page has no room for that way, and no
/// Windows folder name holds a `?`.
pub fn moved(dir: &Path, recorded: &[PathBuf], is_install: impl Fn(&Path) -> bool) -> Option<Moved> {
    let readable = |r: &&PathBuf| !r.to_string_lossy().contains(['?', '\u{fffd}']);
    let recorded: Vec<&PathBuf> = recorded.iter().filter(readable).collect();
    if recorded.is_empty() || recorded.iter().any(|r| same_folder(r, dir)) {
        return None;
    }
    let there = recorded.iter().find(|r| is_install(r));
    Some(Moved {
        dir: dir.to_path_buf(),
        recorded: (*there.unwrap_or(&recorded[0])).clone(),
        there: there.is_some(),
    })
}

#[cfg(windows)]
pub(super) fn program_files() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect()
}

/// The folders the EA app installs games into, from the `downloadinplacedir`
/// lines of its `machine.ini` and `user_<id>.ini` settings.
pub fn library_dirs(ini: &str) -> Vec<PathBuf> {
    ini.lines()
        .filter_map(|l| l.trim_end_matches('\r').split_once('='))
        .filter(|(k, _)| k.trim().to_ascii_lowercase().ends_with(".downloadinplacedir"))
        .map(|(_, v)| v.trim().trim_matches('"'))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(windows)]
pub(super) fn settings_files() -> Vec<PathBuf> {
    let machine = std::env::var_os("ProgramData").map(|d| PathBuf::from(d).join(r"EA Desktop\machine.ini"));
    let users = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join(r"Electronic Arts\EA Desktop"))
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("user_") && n.ends_with(".ini"))
        });
    machine.into_iter().chain(users).collect()
}

#[cfg(windows)]
pub fn installs() -> Vec<PathBuf> {
    let defaults: Vec<PathBuf> = settings_files()
        .into_iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .flat_map(|ini| library_dirs(&ini))
        .chain(program_files().into_iter().map(|p| p.join("EA Games")))
        .collect();
    find_installs(read_reg, &defaults, is_install)
}

/// Linux has no standard place for the EA app's games; players add the
/// folder in the launcher.
#[cfg(not(windows))]
pub fn installs() -> Vec<PathBuf> {
    Vec::new()
}
