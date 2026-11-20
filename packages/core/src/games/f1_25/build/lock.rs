use super::LOCK_FILE;
use crate::store::Store;
use crate::{Result, io_err};

/// Held while a build writes a profile's overlay; another build of the
/// profile, in this process or another, waits until it is dropped.
pub struct BuildLock {
    _file: std::fs::File,
}

pub fn lock(store: &Store, profile: &str) -> Result<BuildLock> {
    let dir = store.overlays_dir(profile);
    std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let path = dir.join(LOCK_FILE);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io_err(&path))?;
    file.lock().map_err(io_err(&path))?;
    Ok(BuildLock { _file: file })
}
