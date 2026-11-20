use std::path::{Path, PathBuf};

use crate::game_folder::baseline::sha256_file;
use crate::platform::process::same_folder;

use super::Proxy;

pub(super) fn proxy(path: &Path, placed: Option<&str>) -> Proxy {
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Proxy::Missing,
        Err(e) => return Proxy::Unreadable(e.to_string()),
    };
    match sha256_file(path) {
        Ok(sha) => Proxy::Present {
            size,
            same: placed.map(|p| p.eq_ignore_ascii_case(&sha)),
            zone: zone(path),
        },
        Err(e) => Proxy::Unreadable(e.to_string()),
    }
}

/// The ZoneId of a file's Mark of the Web (3 is the internet).
fn zone(path: &Path) -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    zone_id(&std::fs::read_to_string(PathBuf::from(stream)).ok()?)
}

pub(super) fn zone_id(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("ZoneId=")?.trim().parse().ok())
}

pub(super) fn norm(path: &str) -> String {
    let p = path.strip_prefix(r"\\?\").unwrap_or(path);
    if p.as_bytes().get(1) == Some(&b':') || p.starts_with('\\') {
        p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
    } else {
        p.trim_end_matches('/').to_owned()
    }
}

/// Whether `path` is inside `dir`, whatever its case and slashes.
pub(super) fn under(path: &str, dir: &str) -> bool {
    let (p, d) = (norm(path), norm(dir));
    let sep = if d.contains('\\') { '\\' } else { '/' };
    !d.is_empty() && p.len() > d.len() && p.starts_with(&d) && p[d.len()..].starts_with(sep)
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Whether `path` is a file directly in `dir`.
pub(super) fn in_folder(path: &str, dir: &Path) -> bool {
    let cut = path.rfind(['\\', '/']).unwrap_or(0);
    same_folder(Path::new(&path[..cut]), dir)
}

/// A path with the user's home folder shortened, so no user name is logged.
pub(super) fn tidy(path: &str) -> String {
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok();
    tidy_with(path, home.as_deref())
}

pub(super) fn tidy_with(path: &str, home: Option<&str>) -> String {
    let short = if cfg!(windows) { "%USERPROFILE%" } else { "~" };
    let p = path.strip_prefix(r"\\?\").unwrap_or(path);
    let Some(h) = home
        .map(|h| h.trim_end_matches(['\\', '/']))
        .filter(|h| h.len() > 3)
    else {
        return path.to_owned();
    };
    let slashes = |s: &str| s.replace('/', "\\");
    match p.get(..h.len()).zip(p.get(h.len()..)) {
        Some((head, rest))
            if slashes(head).eq_ignore_ascii_case(&slashes(h)) && rest.starts_with(['\\', '/']) =>
        {
            format!("{short}{rest}")
        }
        _ => path.to_owned(),
    }
}
