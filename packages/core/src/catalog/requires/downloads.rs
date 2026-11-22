//! The downloads in the shared downloads folder that installs running at once use together.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

use super::install::Event;
use crate::Result;
use crate::catalog::{Addon, Version, download, file_name};

pub(super) type Zips = RefCell<BTreeMap<(String, semver::Version), PathBuf>>;

/// A download in the shared downloads folder that installs running at once use
/// together: one of them downloads it while the others wait, and the last one
/// done with it deletes it.
#[derive(Default)]
pub(super) struct Shared {
    pub(super) users: usize,
    pub(super) busy: bool,
    pub(super) ready: bool,
}

pub(super) static SHARED: Mutex<BTreeMap<PathBuf, Shared>> = Mutex::new(BTreeMap::new());

pub(super) static SHARED_CHANGED: Condvar = Condvar::new();

pub(super) static PART: AtomicU64 = AtomicU64::new(0);

pub(super) fn shared() -> MutexGuard<'static, BTreeMap<PathBuf, Shared>> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn fetch(
    dir: &Path,
    zips: &Zips,
    addon: &Addon,
    v: &Version,
    progress: &dyn Fn(Event),
) -> Result<PathBuf> {
    let key = (addon.id.clone(), v.version.clone());
    if let Some(path) = zips.borrow().get(&key) {
        return Ok(path.clone());
    }
    let path = dir.join(file_name(addon, v));
    zips.borrow_mut().insert(key, path.clone());
    let mut all = shared();
    all.entry(path.clone()).or_default().users += 1;
    loop {
        let slot = all.get_mut(&path).expect("held while this install uses it");
        if slot.ready {
            return Ok(path);
        }
        if !slot.busy {
            slot.busy = true;
            break;
        }
        all = SHARED_CHANGED.wait(all).unwrap_or_else(|e| e.into_inner());
    }
    drop(all);
    progress(Event::Addon(addon, v));
    let part = path.with_extension(format!(
        "zip.part-{}-{}",
        std::process::id(),
        PART.fetch_add(1, Ordering::Relaxed)
    ));
    let result = download(v, &part, &|s| progress(Event::Stage(s)))
        .and_then(|()| crate::with_retry(&path, || std::fs::rename(&part, &path)));
    if result.is_err() {
        std::fs::remove_file(&part).ok();
    }
    let mut all = shared();
    if let Some(slot) = all.get_mut(&path) {
        slot.busy = false;
        slot.ready = result.is_ok();
    }
    SHARED_CHANGED.notify_all();
    result.map(|()| path)
}

/// Lets go of the downloads one install used, deleting each that no other
/// running install still uses.
pub(super) fn release(zips: Zips) {
    let mut all = shared();
    for path in zips.into_inner().into_values() {
        let Some(slot) = all.get_mut(&path) else {
            continue;
        };
        slot.users -= 1;
        if slot.users == 0 {
            all.remove(&path);
            std::fs::remove_file(&path).ok();
        }
    }
    SHARED_CHANGED.notify_all();
}
