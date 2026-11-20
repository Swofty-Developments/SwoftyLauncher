//! Putting an addon's new copy in place of its old one: the old copy set aside and deleted once nothing holds
//! it, a swap another process is making waited out, and a swap stopped part way put back.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Store;
use super::scan::manifest_id;
use crate::manifest::AddonManifest;
use crate::{Error, Result};

/// Where an addon's old copy waits to be deleted, next to the addons.
pub(super) const OLD_PREFIX: &str = ".old-";

pub(super) const STAGING_PREFIX: &str = ".installing-";

/// Where an archive is unpacked before its addons are installed.
pub(super) const SCRATCH_PREFIX: &str = ".importing-";

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

/// The addon's [`SwapLock`]. A lock still held a moment after this process let it go, as a
/// process starting a helper holds it until the helper runs, is waited out.
pub(super) fn swap_lock_for(dest: &Path, name: &str) -> Result<Option<SwapLock>> {
    const DELAYS_MS: [u64; 3] = [10, 50, 150];
    let (Some(addons), Some(id)) = (dest.parent(), dest.file_name()) else {
        return Ok(None);
    };
    let mut attempt = 0;
    loop {
        match SwapLock::try_acquire(addons, &id.to_string_lossy()) {
            Some(None) if attempt < DELAYS_MS.len() => {
                std::thread::sleep(std::time::Duration::from_millis(DELAYS_MS[attempt]));
                attempt += 1;
            }
            Some(None) => {
                return Err(Error::Other(format!(
                    "Couldn't change {name}: another Swofty launcher is changing it right now"
                )));
            }
            Some(Some(lock)) => return Ok(Some(lock)),
            None => return Ok(None),
        }
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

/// [`swap_in_held`] under the addon's [`SwapLock`].
#[cfg(test)]
pub(super) fn swap_in(staging: &Path, dest: &Path, name: &str, rename: Rename, delete: Delete) -> Result<()> {
    let _lock = swap_lock_for(dest, name)?;
    swap_in_held(staging, dest, name, rename, delete)
}

/// Puts `staging` in place of `dest` for a caller that holds the addon's
/// [`SwapLock`]. The installed copy is moved aside first and deleted last, so a
/// failure at any step leaves it whole. An old copy that can't be deleted yet is
/// deleted on the next start; one that can't be put back is put back then.
pub(super) fn swap_in_held(
    staging: &Path,
    dest: &Path,
    name: &str,
    rename: Rename,
    delete: Delete,
) -> Result<()> {
    let verb = if dest.exists() { "update" } else { "install" };
    let what = format!("Couldn't {verb} {name}");
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

/// A folder an archive is unpacked into before its addons are installed, named for this
/// process. Its lock tells a later start's cleanup that it is in use; dropping it deletes the
/// folder and the lock.
pub(super) struct Scratch {
    pub(super) dir: PathBuf,
    lock: Option<SwapLock>,
}

impl Scratch {
    pub(super) fn new(addons: &Path) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("{SCRATCH_PREFIX}{}-{n}", std::process::id());
        std::fs::create_dir_all(addons).ok();
        let lock = SwapLock::try_acquire(addons, &name).flatten();
        Self {
            dir: addons.join(name),
            lock,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
        let held = self.lock.take().is_some();
        if held && let (Some(addons), Some(name)) = (self.dir.parent(), self.dir.file_name()) {
            std::fs::remove_file(swap_lock_path(addons, &name.to_string_lossy())).ok();
        }
    }
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

    /// Deletes the unpack and staging folders of installs an earlier run left unfinished,
    /// which leaves each addon at the version it had. A folder whose install is still running
    /// in another launcher is left alone.
    pub(super) fn sweep_unfinished(&self) -> Vec<String> {
        let addons = self.addons_dir();
        let Ok(rd) = std::fs::read_dir(&addons) else {
            return Vec::new();
        };
        let mut left: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| {
                p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with(SCRATCH_PREFIX) || n.starts_with(STAGING_PREFIX)
                })
            })
            .collect();
        left.sort();
        let mut notes = Vec::new();
        for dir in left {
            let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let staged = name.strip_prefix(STAGING_PREFIX);
            let key = staged.unwrap_or(&name);
            let guard = match SwapLock::try_acquire(&addons, key) {
                Some(None) => continue,
                guard => guard.flatten(),
            };
            if std::fs::remove_dir_all(&dir).is_err() {
                continue;
            }
            notes.push(match staged {
                Some(id) => {
                    let kept = AddonManifest::load(&addons.join(id)).map_or_else(
                        |_| "is not installed".to_owned(),
                        |m| format!("stays at {}", m.version),
                    );
                    format!("deleted the unfinished install of {id}, which {kept}")
                }
                None => format!("deleted the unfinished unpack {name}"),
            });
            if staged.is_none() && guard.is_some() {
                drop(guard);
                std::fs::remove_file(swap_lock_path(&addons, key)).ok();
            }
        }
        notes
    }
}
