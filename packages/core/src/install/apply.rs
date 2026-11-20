//! Applying a plan: the room it needs on each volume, and placing it step by step.

use std::path::{Path, PathBuf};

use super::checks::{hashed, refuse_drop_ins};
use super::copy::{copy_report, sources, through_cache};
use super::{Installer, Plan};
use crate::cache::link::Pending;
use crate::diagnostics::phases;
use crate::game_folder::baseline::{Baseline, Hashes};
use crate::game_folder::placed::Placed;
use crate::{Error, Result, io_err, with_retry};

/// Room on the data folder's volume per file an install places, for the
/// journal, `placed.json` and the hash cache, each rewritten through a
/// temporary copy.
pub(super) const STATE_PER_FILE: u64 = 2048;

impl<'a> Installer<'a> {
    pub fn apply(&self, plan: &Plan) -> Result<()> {
        let mut hashes = phases::time("install.hashes_load", || Hashes::load(self.data));
        let out = self.apply_with(plan, &mut hashes);
        phases::end_step();
        phases::time("install.hashes_save", || hashes.save());
        out
    }

    pub(super) fn apply_with(&self, plan: &Plan, hashes: &mut Hashes) -> Result<()> {
        self.roll_back_with(false, hashes)?;
        // A live shader config left behind by a session that ended without its rollback is the launcher's
        // own; no session runs during an apply.
        let live = self.path(crate::content::shaders::LIVE_FILE);
        if live.is_file() {
            with_retry(&live, || crate::remove_placed(&live))?;
            for dir in live.ancestors().skip(1).take(2) {
                if std::fs::remove_dir(dir).is_err() {
                    break;
                }
            }
        }
        if plan.is_vanilla() {
            return Ok(());
        }
        let mut with_state;
        let plan = if plan.loader.is_empty() {
            plan
        } else {
            let file = self.data.join("state").join("core-state.json");
            let dir = file.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            std::fs::write(&file, crate::stdlib::state::text(self.data)).map_err(io_err(&file))?;
            with_state = plan.clone();
            with_state
                .loader
                .insert(crate::stdlib::state::FILE.to_owned(), file);
            &with_state
        };
        let started = std::time::Instant::now();
        let loaded = self.baseline()?;
        let space = self.space(plan, &loaded);
        let need = self.need(plan, &loaded, hashes).unwrap_or_else(|| space.clone());
        phases::record("install.baseline", started.elapsed());
        for (dir, v, need) in &need {
            if let Some(free) = (self.free_space)(dir).filter(|free| need > free) {
                return Err(Error::NoSpace {
                    drive: v.name.clone(),
                    need: *need,
                    free,
                });
            }
        }
        let stash = self.room_for_originals(plan, &space);
        self.place(plan, hashes, loaded, stash)
            .map_err(|e| self.full(e, &space))
    }

    /// Whether the game's drive has room to keep the game files `plan` replaces until the rollback, beside
    /// everything the install writes and the link cache's spare room. Without it each replaced file is
    /// deleted, as its backup holds it.
    pub(super) fn room_for_originals(
        &self,
        plan: &Plan,
        space: &[(PathBuf, crate::platform::disk::Volume, u64)],
    ) -> bool {
        let kept: u64 = plan
            .data
            .keys()
            .map(|rel| std::fs::metadata(self.path(rel)).map_or(0, |m| m.len()))
            .sum();
        let need: u64 = space.iter().map(|(_, _, need)| need).sum();
        (self.free_space)(&self.game.dir).is_some_and(|free| {
            free >= need
                .saturating_add(kept)
                .saturating_add(crate::cache::link::MARGIN)
        })
    }

    /// The bytes an install of `plan` writes on each volume when it copies
    /// every file: its copies, less the files they replace, on the game
    /// folder's; the originals it backs up for the first time and its state
    /// files on the data folder's.
    pub(super) fn space(
        &self,
        plan: &Plan,
        baseline: &Baseline,
    ) -> Vec<(PathBuf, crate::platform::disk::Volume, u64)> {
        let game = sources(plan)
            .into_iter()
            .map(|(rel, src)| self.copied(plan, rel, src))
            .sum();
        self.on_volumes(game, self.backed_up(plan, baseline))
    }

    /// The free space an install of `plan` that links its files needs on each volume: [`Self::space`], except
    /// on the game folder's, where it is what [`Pending::need`] says for the files it links from the link
    /// cache. Those linked straight from the build output on that drive need none. None when it copies
    /// every file, or the drives can't be told apart, and needs [`Self::space`].
    pub(super) fn need(
        &self,
        plan: &Plan,
        baseline: &Baseline,
        hashes: &mut Hashes,
    ) -> Option<Vec<(PathBuf, crate::platform::disk::Volume, u64)>> {
        let (pending, copying) = self.linking(plan, hashes)?;
        Some(self.on_volumes(pending.need(copying), self.backed_up(plan, baseline)))
    }

    /// For the files the install of `plan` links from the link cache: what the sync of the cache does, and
    /// the bytes copying them straight into the game folder adds instead. None when the install copies every
    /// file, or the drives can't be told apart.
    pub(super) fn linking(&self, plan: &Plan, hashes: &mut Hashes) -> Option<(Pending, u64)> {
        if self.copy_only {
            return None;
        }
        let built = crate::store::overlay_root(self.data);
        let apart = crate::cache::link::apart((self.volume)(&built), (self.volume)(&self.game.dir))?;
        let cached: Vec<(&str, &Path)> = sources(plan)
            .into_iter()
            .filter(|(_, src)| through_cache(Some(apart), &built, src))
            .map(|(rel, src)| (rel, src.as_path()))
            .collect();
        if cached.is_empty() {
            return Some((Pending::default(), 0));
        }
        let copying = cached.iter().map(|(rel, src)| self.copied(plan, rel, src)).sum();
        let pending =
            crate::cache::link::pending(&self.game.dir, &cached, &mut |src| hashes.sha256(src).ok());
        phases::note(format!(
            "space link cache copies {} MB of {} file(s), frees {} MB; copying {} MB",
            pending.copies >> 20,
            cached.len(),
            pending.frees >> 20,
            copying >> 20
        ));
        Some((pending, copying))
    }

    /// The bytes copying `src` to the game path `rel` adds on the game's drive: its size, less the game file
    /// it replaces.
    fn copied(&self, plan: &Plan, rel: &str, src: &Path) -> u64 {
        let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
        let replaced = if plan.data.contains_key(rel) {
            size(&self.path(rel))
        } else {
            0
        };
        size(src).saturating_sub(replaced)
    }

    /// The bytes an install of `plan` writes on the data folder's volume: the originals it backs up for the
    /// first time and its state files.
    fn backed_up(&self, plan: &Plan, baseline: &Baseline) -> u64 {
        let originals: u64 = plan
            .data
            .keys()
            .filter(|rel| !baseline.files.contains_key(*rel))
            .map(|rel| std::fs::metadata(self.path(rel)).map_or(0, |m| m.len()))
            .sum();
        originals + (plan.data.len() + plan.added.len() + plan.loader.len()) as u64 * STATE_PER_FILE
    }

    /// `game` bytes on the game folder's volume and `data` on the data folder's, added up when they are one.
    fn on_volumes(&self, game: u64, data: u64) -> Vec<(PathBuf, crate::platform::disk::Volume, u64)> {
        crate::platform::disk::volumes([(self.game.dir.as_path(), game), (self.data, data)])
    }

    /// A write that failed on a full volume, as the volume and the room the
    /// install needed there.
    pub(super) fn full(&self, e: Error, space: &[(PathBuf, crate::platform::disk::Volume, u64)]) -> Error {
        let Error::Io { path, source } = &e else {
            return e;
        };
        let on = crate::platform::disk::volume(path).filter(|_| crate::platform::disk::is_full(source));
        match on.and_then(|on| space.iter().find(|(_, v, _)| v.id == on.id)) {
            Some((dir, v, need)) => Error::NoSpace {
                drive: v.name.clone(),
                need: *need,
                free: (self.free_space)(dir).unwrap_or(0),
            },
            None => e,
        }
    }

    pub(super) fn place(
        &self,
        plan: &Plan,
        hashes: &mut Hashes,
        loaded: Baseline,
        stash: bool,
    ) -> Result<()> {
        crate::now_doing("checking the game files");
        refuse_drop_ins(plan)?;
        let objects = self.objects();
        let mut baseline = loaded.clone();
        let mut placed = Placed::load(self.data)?;
        let after = hashed(plan, hashes)?;
        self.put_back_missing(plan, &baseline, &objects)?;
        let (mut conflicts, unproven) =
            self.check_replacements(plan, hashes, &mut baseline, &objects, &placed, &after)?;
        self.check_unproven(&mut baseline, &unproven)?;
        if baseline != loaded {
            baseline.save(self.data)?;
        }
        conflicts.extend(self.check_new_files(plan, hashes, &baseline, &placed, &after)?);
        if !conflicts.is_empty() {
            return Err(Error::Leftovers(conflicts));
        }
        let (mut j, registry) = self.open_journal(plan, &baseline, &objects, &mut placed, &after)?;
        let sources = sources(plan);
        crate::now_doing("copying the mods into the game folder");
        let copied = self.copy_entries(&mut j, &sources, stash, hashes, registry.as_ref())?;
        (self.out)(&copy_report(&copied, self.copy_only));
        self.put_back_storage(plan, &sources)?;
        if !plan.loader.is_empty() {
            for line in crate::stdlib::state::migrate(self.data, &self.game.dir)? {
                (self.out)(&line);
            }
            for line in crate::platform::run_as_admin::open_core_caches(&self.game.dir) {
                (self.out)(&line);
            }
        }
        j.commit()
    }
}
