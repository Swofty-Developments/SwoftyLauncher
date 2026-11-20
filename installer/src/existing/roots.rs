use std::path::{Path, PathBuf};

use swofty_core::platform::process::same_folder;
use swofty_core::updates::release as update;

use crate::existing::lnk::lnk_roots;
use crate::existing::{default_root, is_install};
use crate::shortcut;

pub(super) fn find() -> Vec<PathBuf> {
    let linked: Vec<PathBuf> = shortcut::files().iter().flat_map(|f| shortcut_roots(f)).collect();
    collect(running_roots().into_iter().chain(linked).chain([default_root()]))
}

pub(super) fn collect(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for root in candidates {
        if is_install(&root) && !found.iter().any(|f| same_folder(f, &root)) {
            found.push(root);
        }
    }
    found
}

pub(super) fn root_of(exe: &Path) -> Option<PathBuf> {
    let bin = exe
        .parent()
        .filter(|b| b.file_name().is_some_and(|n| n.eq_ignore_ascii_case("bin")))?;
    bin.parent().map(Path::to_path_buf)
}

pub(super) fn running_roots() -> Vec<PathBuf> {
    running_launchers()
        .iter()
        .filter_map(|exe| root_of(exe))
        .collect()
}

#[cfg(target_os = "linux")]
fn running_launchers() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let name = update::exe("swofty-launcher");
    entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|e| std::fs::read_link(e.path().join("exe")).ok())
        .map(
            |exe| match exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")) {
                Some(live) => PathBuf::from(live),
                None => exe,
            },
        )
        .filter(|exe| exe.file_name().is_some_and(|n| *n == *name))
        .collect()
}

#[cfg(windows)]
fn running_launchers() -> Vec<PathBuf> {
    crate::win32::images(&update::exe("swofty-launcher"))
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn running_launchers() -> Vec<PathBuf> {
    Vec::new()
}

fn shortcut_roots(file: &Path) -> Vec<PathBuf> {
    let Ok(data) = std::fs::read(file) else {
        return Vec::new();
    };
    if cfg!(windows) {
        lnk_roots(&data)
    } else {
        desktop_roots(&String::from_utf8_lossy(&data))
    }
}

pub(super) fn remove_stale(old: &Path) {
    for file in shortcut::files() {
        if shortcut_roots(&file).iter().any(|r| same_folder(r, old)) {
            std::fs::remove_file(&file).ok();
        }
    }
}

pub(super) fn desktop_roots(text: &str) -> Vec<PathBuf> {
    let value = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='));
    let exe = value("Exec")
        .and_then(first_arg)
        .and_then(|exe| root_of(Path::new(&exe)));
    let dir = value("Path").filter(|p| !p.is_empty()).map(PathBuf::from);
    exe.into_iter().chain(dir).collect()
}

fn first_arg(exec: &str) -> Option<String> {
    let exec = exec.trim_start();
    let Some(quoted) = exec.strip_prefix('"') else {
        return exec.split_whitespace().next().map(str::to_owned);
    };
    let mut out = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            '%' => {
                chars.next();
                out.push('%');
            }
            c => out.push(c),
        }
    }
    None
}
