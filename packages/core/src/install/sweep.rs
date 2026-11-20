//! Clearing the files in the way of a plan, the files earlier sessions left behind, the folders they emptied
//! and old logs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::files::{rel_parts, remove_empty};
use super::wine_override::remove_override;
use super::{Conflict, Installer, LOADER_DIR};
use crate::game_folder::baseline::Hashes;
use crate::game_folder::journal::{Journal, Op};
use crate::game_folder::placed::{self, Placed};
use crate::{Error, Result, io_err, with_retry};

impl<'a> Installer<'a> {
    /// Clear the conflicts a failed [`apply`](Self::apply) reported that are
    /// provably the launcher's own: originals go back over replaced files and
    /// added files are removed, with any folders that held only them. Anything
    /// not owned is left alone.
    pub fn clear(&self, conflicts: &[Conflict]) -> Result<Vec<String>> {
        let baseline = self.baseline()?;
        let mut placed = Placed::load(self.data)?;
        let mut notes = Vec::new();
        for c in conflicts.iter().filter(|c| c.owned) {
            let path = self.path(&c.path);
            if c.replaced {
                let rec = baseline
                    .files
                    .get(&c.path)
                    .ok_or_else(|| Error::Other(format!("{} has no backed-up original", c.path)))?;
                self.restore_original(&rec.sha256, &path)?;
                notes.push(format!("put the original {} back", c.path));
            } else {
                if path.is_file() {
                    with_retry(&path, || std::fs::remove_file(&path))?;
                }
                self.prune_dirs(&c.path)?;
                notes.push(format!("removed {}", c.path));
            }
            placed.remove(&c.path);
        }
        placed.save(self.data)?;
        Ok(notes)
    }

    /// Clean up files the placed-files record says the launcher wrote and no
    /// open journal accounts for: a session that lost its journal left them.
    /// Only files that still hold exactly the recorded bytes are touched.
    pub fn sweep(&self) -> Result<Vec<String>> {
        let mut hashes = Hashes::load(self.data);
        let result = self.sweep_with(&mut hashes);
        hashes.save();
        result
    }

    pub(super) fn sweep_with(&self, hashes: &mut Hashes) -> Result<Vec<String>> {
        let mut placed = Placed::load(self.data)?;
        let open = Journal::open(self.data)?;
        let originals = crate::cache::link::originals(&self.game.dir);
        if open.is_empty() && originals.is_dir() {
            std::fs::remove_dir_all(&originals).ok();
            std::fs::remove_dir(crate::cache::link::dir(&self.game.dir)).ok();
        }
        if placed.files.is_empty() && placed.registry.is_none() {
            return Ok(Vec::new());
        }
        let journaled: BTreeSet<String> = open
            .iter()
            .flat_map(|j| j.entries.iter().map(|e| placed::key(&e.path)))
            .collect();
        let baseline = self.baseline()?;
        let mut notes = Vec::new();
        let overrides = open
            .iter()
            .flat_map(|j| &j.entries)
            .any(|e| e.op == Op::RegistryOverride);
        if let Some(reg) = placed.registry.clone().filter(|_| !overrides) {
            let path = PathBuf::from(&reg);
            if let Some(out) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| remove_override(&t))
            {
                std::fs::write(&path, out).map_err(io_err(&path))?;
                notes.push("removed the version.dll override from the Proton prefix".to_owned());
            }
            placed.registry = None;
        }
        for (key, f) in placed.files.clone() {
            if journaled.contains(&key) {
                continue;
            }
            let path = self.path(&f.path);
            if hashes.sha256_opt(&path)?.as_deref() == Some(f.sha256.as_str()) {
                if f.replaced {
                    let older = || {
                        crate::game_folder::baseline::older_originals(self.data, &self.game.identity, &f.path)
                            .into_iter()
                            .next()
                            .map(|(_, rec)| rec)
                    };
                    match baseline.files.get(&f.path).cloned().or_else(older) {
                        Some(rec) if self.objects().has(&rec.sha256) => {
                            self.put_original(&rec.sha256, &path, hashes)?;
                            notes.push(format!("put the original {} back", f.path));
                        }
                        _ => {
                            notes.push(format!(
                                "{} holds an unfinished session's file and has no backed-up original; left in place",
                                f.path
                            ));
                            continue;
                        }
                    }
                } else {
                    with_retry(&path, || std::fs::remove_file(&path))?;
                    self.prune_dirs(&f.path)?;
                    notes.push(format!("removed {}", f.path));
                }
            }
            placed.files.remove(&key);
        }
        placed.save(self.data)?;
        Ok(notes)
    }

    pub(super) fn prune_dirs(&self, rel: &str) -> Result<()> {
        for dir in rel_parts(rel).iter().rev() {
            let path = self.path(dir);
            let empty = path.is_dir() && std::fs::read_dir(&path).map_err(io_err(&path))?.next().is_none();
            if !empty {
                break;
            }
            with_retry(&path, || std::fs::remove_dir(&path))?;
        }
        Ok(())
    }

    /// Move what the session left in the loader folder (its logs, diagnostic
    /// dumps, load-stall reports and crash reports with their minidumps) to `logs`, and remove the
    /// folders it leaves empty, once no journal is open. Returns how many files
    /// moved.
    pub fn tidy(&self, logs: &Path) -> Result<usize> {
        let loader = self.path(LOADER_DIR);
        let prefix = format!("{LOADER_DIR}/");
        if !loader.is_dir()
            || !Journal::open(self.data)?.is_empty()
            || self.baseline()?.files.keys().any(|k| k.starts_with(&prefix))
        {
            return Ok(0);
        }
        let status = self.path(crate::saves::launcher::STATUS_FILE);
        if status.is_file() {
            with_retry(&status, || std::fs::remove_file(&status))?;
        }
        let mut moved = 0;
        for e in std::fs::read_dir(&loader).map_err(io_err(&loader))?.flatten() {
            let path = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            let crash = crate::diagnostics::crash_report::is_crash_name(&name)
                || crate::diagnostics::crash_report::is_stall_name(&name);
            let output = crash
                || path
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("log") || x.eq_ignore_ascii_case("dump"));
            if output && path.is_file() {
                std::fs::create_dir_all(logs).map_err(io_err(logs))?;
                crate::move_file(&path, &logs.join(e.file_name()))?;
                moved += 1;
            }
        }
        remove_empty(&loader)?;
        Ok(moved)
    }
}
