//! The game's original files an install replaces, kept aside by their hash and put back.

use std::path::{Path, PathBuf};

use super::Installer;
use crate::game_folder::baseline::Hashes;
use crate::{Error, Result};

impl<'a> Installer<'a> {
    pub(super) fn restore_original(&self, sha: &str, path: &Path) -> Result<()> {
        const DELAYS_MS: [u64; 5] = [50, 150, 300, 600, 1000];
        let mut attempt = 0;
        loop {
            match self.objects().restore_to(sha, path) {
                Err(Error::Io { source, .. })
                    if crate::is_locked_file_error(&source) && attempt < DELAYS_MS.len() =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(DELAYS_MS[attempt]));
                    attempt += 1;
                }
                r => return r,
            }
        }
    }

    /// Where an install keeps the game file holding `sha` that it replaced, on the game's drive.
    pub(super) fn stashed(&self, sha: &str) -> PathBuf {
        crate::cache::link::originals(&self.game.dir).join(sha)
    }

    /// Moves the game file at `path`, which holds `sha`, into the originals folder, so the rollback can
    /// rename it back. False when it stays where it is, as on a game folder that spans drives; the install
    /// then replaces it, and the rollback copies it back from the backup.
    pub(super) fn stash(&self, path: &Path, sha: &str, hashes: &mut Hashes) -> bool {
        let at = self.stashed(sha);
        let moved = at
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::rename(path, &at));
        if moved.is_ok() {
            hashes.wrote(&at, sha);
        }
        moved.is_ok()
    }

    /// Renames the original holding `sha` from the originals folder over `path`, once its bytes are proven
    /// to be that original. False when it is not there or no longer holds them.
    pub(super) fn unstash(&self, sha: &str, path: &Path, hashes: &mut Hashes) -> bool {
        let at = self.stashed(sha);
        if !hashes
            .sha256_opt(&at)
            .is_ok_and(|now| now.as_deref() == Some(sha))
        {
            return false;
        }
        let renamed = match std::fs::rename(&at, path) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && crate::clear_readonly(path) => {
                std::fs::rename(&at, path)
            }
            r => r,
        };
        if renamed.is_err() {
            return false;
        }
        crate::sync_dir(path.parent());
        true
    }

    /// Puts the original holding `sha` back at `path`: renamed back from the originals folder when the
    /// install moved it there and it is unchanged, else copied from the backup. A backup is never linked
    /// into the game folder, where a write through the link would change it.
    pub(super) fn put_original(&self, sha: &str, path: &Path, hashes: &mut Hashes) -> Result<()> {
        if !self.unstash(sha, path, hashes) {
            self.restore_original(sha, path)?;
        }
        hashes.wrote(path, sha);
        Ok(())
    }

    /// Deletes what is left in the originals folder of `shas`, then the folder and the cache folder once
    /// they are empty. What is left there is a copy of a backup, so this only ever costs speed.
    pub(super) fn drop_originals<'s>(&self, shas: impl IntoIterator<Item = &'s str>) {
        for sha in shas {
            let at = self.stashed(sha);
            if at.is_file() {
                std::fs::remove_file(&at).ok();
            }
        }
        if std::fs::remove_dir(crate::cache::link::originals(&self.game.dir)).is_ok() {
            std::fs::remove_dir(crate::cache::link::dir(&self.game.dir)).ok();
        }
    }
}
