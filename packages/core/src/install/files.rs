//! File steps of an install: a file copied over another, a partial copy dropped, a game path's folders, empty
//! folders removed, the proxy the launcher ships, and a file described for a message.

use std::path::Path;

use super::PROXY;
use crate::game_folder::baseline::sha256_opt;
use crate::{Result, io_err, with_retry};

pub(super) fn quiet(_: &str) {}

pub(super) fn unwatched(_: usize, _: usize) {}

pub(super) fn link_replacing(src: &Path, path: &Path) -> std::io::Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}.swofty-link",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::remove_file(&tmp).ok();
    std::fs::hard_link(src, &tmp)?;
    let renamed = match std::fs::rename(&tmp, path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && crate::clear_readonly(path) => {
            std::fs::rename(&tmp, path)
        }
        r => r,
    };
    if renamed.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    renamed
}

/// Deletes the temporary files a write of `path` that a crash cut short left beside it.
pub(super) fn drop_partial(path: &Path) {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    for kind in ["swofty-link", "swofty-copy", "swofty-restore"] {
        let tmp = path.with_file_name(format!(".{name}.{kind}"));
        if tmp.is_file() {
            std::fs::remove_file(&tmp).ok();
        }
    }
}

pub(crate) fn copy_replacing(src: &Path, path: &Path) -> std::io::Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}.swofty-copy",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::remove_file(&tmp).ok();
    if let Err(e) = std::fs::copy(src, &tmp) {
        std::fs::remove_file(&tmp).ok();
        return Err(e);
    }
    let renamed = match std::fs::rename(&tmp, path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && crate::clear_readonly(path) => {
            std::fs::rename(&tmp, path)
        }
        r => r,
    };
    if renamed.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    renamed
}

pub(super) fn rel_parts(rel: &str) -> Vec<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    (1..parts.len()).map(|i| parts[..i].join("/")).collect()
}

/// Whether `path` holds, byte for byte, the proxy this launcher ships. A
/// proxy the launcher's records account for is gone before this is asked:
/// the rollback and the sweep removed it.
pub(super) fn shipped_proxy(path: &Path) -> Result<bool> {
    let Some(shipped) =
        crate::addons::builtin::loader_dir().and_then(|d| sha256_opt(&d.join(PROXY)).ok().flatten())
    else {
        return Ok(false);
    };
    Ok(sha256_opt(path)?.is_some_and(|now| now == shipped))
}

pub(super) fn remove_empty(dir: &Path) -> Result<bool> {
    let mut empty = true;
    for e in std::fs::read_dir(dir).map_err(io_err(dir))?.flatten() {
        let path = e.path();
        let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
        if !is_dir || !remove_empty(&path)? {
            empty = false;
        }
    }
    if empty {
        with_retry(dir, || std::fs::remove_dir(dir))?;
    }
    Ok(empty)
}

pub(super) fn describe_file(path: &Path, sha256: &str) -> String {
    let meta = std::fs::metadata(path).ok();
    let size = meta
        .as_ref()
        .map_or_else(|| "?".to_owned(), |m| m.len().to_string());
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or_else(|| "?".to_owned(), |d| d.as_secs().to_string());
    format!("size {size}, modified {mtime} (unix seconds), sha256 {sha256}")
}
