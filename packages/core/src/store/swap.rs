//! Putting an addon's new copy in place of its old one: the old copy set aside and deleted once nothing holds
//! it, a swap another process is making waited out, and a swap stopped part way put back.

use std::path::{Path, PathBuf};

use super::Store;
use super::scan::manifest_id;
use crate::{Error, Result};

/// Where an addon's old copy waits to be deleted, next to the addons.
pub(super) const OLD_PREFIX: &str = ".old-";

pub(super) const STAGING_PREFIX: &str = ".installing-";

pub(super) type Rename<'a> = &'a mut dyn FnMut(&Path, &Path) -> std::io::Result<()>;

pub(super) type Delete<'a> = &'a mut dyn FnMut(&Path) -> std::io::Result<()>;

/// Runs a folder move or delete, waiting out a file that is still briefly in
/// use once its helper has exited.
pub(super) fn waiting_out_locks(mut op: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    const DELAYS_MS: [u64; 5] = [50, 150, 300, 600, 1000];
    let mut attempt = 0;
    loop {
        match op() {
            Err(e) if crate::is_locked_file_error(&e) && attempt < DELAYS_MS.len() => {
                std::thread::sleep(std::time::Duration::from_millis(DELAYS_MS[attempt]));
                attempt += 1;
            }
            r => return r,
        }
    }
}

pub(super) fn not_done(what: &str, e: &std::io::Error) -> Error {
    Error::Other(if crate::is_locked_file_error(e) {
        format!(
            "{what}: its files are in use. Close other Swofty Launcher windows and F1 25, then try again."
        )
    } else {
        format!("{what}: {e}")
    })
}

pub(super) fn swap_lock_path(addons: &Path, id: &str) -> PathBuf {
    addons.join(format!(".swap-{id}.lock"))
}

/// Held while one process swaps an addon folder, so another launcher's
/// start-up cleanup leaves the half-finished swap alone. The operating system
/// drops it when the process ends, which is how a later start tells a dead
/// swap from a live one. Where the file system cannot lock, swaps go unguarded.
pub(super) struct SwapLock {
    pub(super) _file: std::fs::File,
}

impl SwapLock {
    pub(super) fn try_acquire(addons: &Path, id: &str) -> Option<Option<Self>> {
        let path = swap_lock_path(addons, id);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .ok()?;
        match file.try_lock() {
            Err(std::fs::TryLockError::WouldBlock) => Some(None),
            Ok(()) | Err(std::fs::TryLockError::Error(_)) => Some(Some(Self { _file: file })),
        }
    }
}

pub(super) fn swap_lock_for(dest: &Path, name: &str) -> Result<Option<SwapLock>> {
    let (Some(addons), Some(id)) = (dest.parent(), dest.file_name()) else {
        return Ok(None);
    };
    match SwapLock::try_acquire(addons, &id.to_string_lossy()) {
        Some(None) => Err(Error::Other(format!(
            "Couldn't change {name}: another Swofty launcher is changing it right now"
        ))),
        Some(Some(lock)) => Ok(Some(lock)),
        None => Ok(None),
    }
}

/// Moves `dir` to a hidden `.old-` folder beside it. A file still in use stops
/// the move, and then nothing in `dir` has changed.
pub(super) fn set_aside(dir: &Path, rename: Rename) -> std::io::Result<PathBuf> {
    let parent = dir.parent().unwrap_or(dir);
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let aside = (0..)
        .map(|n| match n {
            0 => parent.join(format!("{OLD_PREFIX}{name}")),
            n => parent.join(format!("{OLD_PREFIX}{name}-{n}")),
        })
        .find(|p| !p.exists())
        .expect("a free name");
    waiting_out_locks(|| rename(dir, &aside))?;
    Ok(aside)
}

/// Puts `staging` in place of `dest`. The installed copy is moved aside first
/// and deleted last, so a failure at any step leaves it whole. An old copy that
/// can't be deleted yet is deleted on the next start; one that can't be put
/// back is put back then.
pub(super) fn swap_in(staging: &Path, dest: &Path, name: &str, rename: Rename, delete: Delete) -> Result<()> {
    let verb = if dest.exists() { "update" } else { "install" };
    let what = format!("Couldn't {verb} {name}");
    let _lock = swap_lock_for(dest, name)?;
    let aside = match dest.exists().then(|| set_aside(dest, &mut *rename)).transpose() {
        Ok(aside) => aside,
        Err(e) => {
            std::fs::remove_dir_all(staging).ok();
            return Err(not_done(&what, &e));
        }
    };
    if let Err(e) = waiting_out_locks(|| rename(staging, dest)) {
        let restored = aside
            .as_deref()
            .is_none_or(|aside| waiting_out_locks(|| rename(aside, dest)).is_ok());
        if restored {
            std::fs::remove_dir_all(staging).ok();
        }
        return Err(not_done(&what, &e));
    }
    if let Some(aside) = aside {
        waiting_out_locks(|| delete(&aside)).ok();
    }
    Ok(())
}

/// Deletes an addon's folder whole or not at all: it is moved aside first, so
/// a file still in use stops the removal before anything is deleted. An old
/// copy that can't be deleted yet is deleted on the next start.
pub(super) fn delete_addon_dir(dir: &Path, name: &str, rename: Rename, delete: Delete) -> Result<()> {
    let _lock = swap_lock_for(dir, name)?;
    let aside = set_aside(dir, rename).map_err(|e| not_done(&format!("Couldn't remove {name}"), &e))?;
    waiting_out_locks(|| delete(&aside)).ok();
    Ok(())
}

impl Store {
    /// Finishes addon updates and removals an earlier run left: an old copy
    /// whose update stopped before the new copy was in place is put back, and
    /// any other old copy is deleted.
    pub(super) fn finish_swaps(&self) -> Vec<String> {
        let addons = self.addons_dir();
        let Ok(rd) = std::fs::read_dir(&addons) else {
            return Vec::new();
        };
        let mut old: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(OLD_PREFIX))
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        old.sort();
        let mut notes = Vec::new();
        for dir in old {
            let stem = dir
                .file_name()
                .map(|n| n.to_string_lossy().trim_start_matches(OLD_PREFIX).to_owned())
                .unwrap_or_default();
            let base = match stem.rsplit_once('-') {
                Some((b, n)) if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) => b.to_owned(),
                _ => stem.clone(),
            };
            let mut names = vec![stem, base];
            names.extend(manifest_id(&dir));
            names.sort();
            names.dedup();
            let mut guards = Vec::new();
            let mut busy = false;
            for n in &names {
                match SwapLock::try_acquire(&addons, n) {
                    Some(None) => busy = true,
                    Some(Some(g)) => guards.push(g),
                    None => {}
                }
            }
            if busy {
                notes.push(format!(
                    "left {} alone: another launcher is changing it",
                    dir.display()
                ));
                continue;
            }
            let interrupted = manifest_id(&dir).filter(|id| {
                !addons.join(id).exists() && addons.join(format!("{STAGING_PREFIX}{id}")).is_dir()
            });
            if let Some(id) = interrupted {
                let to = addons.join(&id);
                if std::fs::rename(&dir, &to).is_ok() {
                    std::fs::remove_dir_all(addons.join(format!("{STAGING_PREFIX}{id}"))).ok();
                    notes.push(format!("put back {id}: its update stopped part way"));
                }
            } else if std::fs::remove_dir_all(&dir).is_ok() {
                notes.push(format!("deleted the old copy {}", dir.display()));
            }
        }
        notes
    }
}
