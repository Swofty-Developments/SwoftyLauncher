//! Where a game's exe is and whether two game folders are the same: a path as Wine shows it, read on this
//! computer, and folder names compared as Windows compares them.

use std::path::{Path, PathBuf};

use crate::GAME_EXE;

pub(super) fn exe_folder(path: &str) -> Option<PathBuf> {
    let cut = path.rfind(['\\', '/'])?;
    path[cut + 1..]
        .eq_ignore_ascii_case(GAME_EXE)
        .then(|| PathBuf::from(&path[..cut.max(1)]))
}

/// A path as Wine shows it (`Z:\home\...`, `D:\F1 25\...`) on this computer:
/// `Z:` is the root, and any other drive is the prefix's `dosdevices` link.
pub(super) fn host_path(windows: &str, prefix: Option<&Path>) -> Option<PathBuf> {
    if windows.starts_with('/') {
        return Some(PathBuf::from(windows));
    }
    let windows = windows.strip_prefix(r"\\?\").unwrap_or(windows);
    let (drive, rest) = windows.split_once(':')?;
    let drive = drive.to_ascii_lowercase();
    if drive.len() != 1 || !rest.starts_with(['\\', '/']) {
        return None;
    }
    let linked = prefix.and_then(|p| {
        let devices = p.join("dosdevices");
        let target = std::fs::read_link(devices.join(format!("{drive}:"))).ok()?;
        Some(devices.join(target))
    });
    let root = linked.or_else(|| (drive == "z").then(|| PathBuf::from("/")))?;
    Some(
        rest.split(['\\', '/'])
            .filter(|c| !c.is_empty())
            .fold(root, |p, c| p.join(c)),
    )
}

/// Whether two game folders are the same: Windows paths match whatever their
/// case and slashes, and links are followed.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    folder_key(a) == folder_key(b) || crate::game_folder::copies::same_dir(a, b)
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    folder_key(a) == folder_key(b)
}

pub(super) fn folder_key(p: &Path) -> String {
    let s = p.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    if s.as_bytes().get(1) == Some(&b':') {
        s.replace('/', "\\").trim_end_matches('\\').to_lowercase()
    } else {
        s.trim_end_matches('/').to_owned()
    }
}
