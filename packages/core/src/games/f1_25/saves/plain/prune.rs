use std::collections::BTreeSet;
use std::path::Path;

use super::Plain;
use crate::game_folder::recovery::SessionLock;
use crate::saves::vault::restore::Running;
use crate::{Result, io_err};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keep {
    pub newest: usize,
}

impl Default for Keep {
    fn default() -> Self {
        Self { newest: 60 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pruned {
    pub removed: Vec<u64>,
    pub skipped: Option<&'static str>,
}

impl Plain {
    /// The generations pruning keeps: the newest `keep.newest`, `CURRENT`, every pinned one, and the newest
    /// save generation of each session.
    pub fn kept(&self, keep: Keep) -> BTreeSet<u64> {
        let gens = self.generations();
        let mut kept: BTreeSet<u64> = gens.iter().take(keep.newest).map(|m| m.generation).collect();
        kept.extend(self.current());
        kept.extend(self.pins().into_keys());
        let mut seen = BTreeSet::new();
        for m in gens.iter().filter(|m| m.event == super::Event::Save) {
            if let Some(s) = &m.session
                && seen.insert(s.clone())
            {
                kept.insert(m.generation);
            }
        }
        kept
    }

    /// Delete the generations outside [`Self::kept`]. Only the launcher prunes, and never while a session
    /// holds the lock or the game runs. An incomplete generation newer than `CURRENT` stays: the core may
    /// still be writing it. One older than `CURRENT` that cannot be read stays too.
    pub fn prune(&self, data: &Path, keep: Keep, running: Running) -> Result<Pruned> {
        if running.game {
            return Ok(Pruned {
                skipped: Some("F1 25 is running"),
                ..Pruned::default()
            });
        }
        let Some(_lock) = SessionLock::try_acquire(data)? else {
            return Ok(Pruned {
                skipped: Some("a session holds the lock"),
                ..Pruned::default()
            });
        };
        let kept = self.kept(keep);
        let valid: BTreeSet<u64> = self.generations().iter().map(|m| m.generation).collect();
        let mut out = Pruned::default();
        for n in valid.difference(&kept) {
            let dir = self.generation_dir(*n);
            std::fs::remove_dir_all(&dir).map_err(io_err(&dir))?;
            out.removed.push(*n);
        }
        Ok(out)
    }
}
