//! Applying a plan: the room it needs, each file placed or linked from the link cache, and the journal kept
//! as it goes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::files::{copy_replacing, describe_file, link_replacing, rel_parts};
use super::wine_override::add_override;
use super::{Conflict, Installer, Plan};
use crate::diagnostics::phases;
use crate::game_folder::baseline::{Baseline, FileRecord, Hashes, Objects, sha256_bytes, sha256_file};
use crate::game_folder::journal::{Entry, Journal, Op};
use crate::game_folder::placed::{self, Placed};
use crate::{Error, Result, addons::storage as addon_storage, io_err, with_retry};

pub(super) const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

/// Room on the data folder's volume per file an install places, for the
/// journal, `placed.json` and the hash cache, each rewritten through a
/// temporary copy.
pub(super) const STATE_PER_FILE: u64 = 2048;

/// Where each placed file is in the link cache by game path, how many files
/// the sync copied and reused, and why there is no cache when it failed.
#[derive(Default)]
pub(super) struct LinkCache {
    pub(super) via: BTreeMap<String, PathBuf>,
    pub(super) synced: Option<(usize, usize)>,
    pub(super) failure: Option<String>,
}

impl<'a> Installer<'a> {
    /// Whether the file at `rel`, which the baseline has no original for, may
    /// be recorded as one. Never when it holds exactly what the launcher wrote
    /// there in place of an original (a session whose journal and baseline
    /// were both lost); and while a damaged baseline is being rebuilt, only
    /// when its bytes are already a backed-up original.
    pub(super) fn may_record(
        &self,
        baseline: &Baseline,
        objects: &Objects,
        placed: &Placed,
        rel: &str,
    ) -> Result<bool> {
        let ours = placed.get(rel).filter(|p| p.replaced);
        if ours.is_none() && !baseline.checking() {
            return Ok(true);
        }
        let now = sha256_file(&self.path(rel))?;
        if ours.is_some_and(|p| p.sha256 == now) {
            return Ok(false);
        }
        Ok(!baseline.checking() || objects.has(&now))
    }

    pub(super) fn step(&self, n: &mut usize) -> Result<()> {
        if self.session {
            crate::game_folder::busy::check(self.data)?;
        }
        *n += 1;
        match self.stop_after {
            Some(limit) if *n > limit => Err(Error::Other(format!("stopped after {limit} step(s)"))),
            _ => Ok(()),
        }
    }

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
        phases::record("install.baseline", started.elapsed());
        for (dir, v, need) in &space {
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

    /// The bytes an install of `plan` writes on each volume: its copies, less
    /// the files they replace, on the game folder's; the originals it backs
    /// up for the first time and its state files on the data folder's.
    pub(super) fn space(
        &self,
        plan: &Plan,
        baseline: &Baseline,
    ) -> Vec<(PathBuf, crate::platform::disk::Volume, u64)> {
        let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
        let (mut game, mut data) = (0, 0);
        for (rel, src) in &plan.data {
            let now = size(&self.path(rel));
            game += size(src).saturating_sub(now);
            if !baseline.files.contains_key(rel) {
                data += now;
            }
        }
        for src in plan.added.values().chain(plan.loader.values()) {
            game += size(src);
        }
        data += (plan.data.len() + plan.added.len() + plan.loader.len()) as u64 * STATE_PER_FILE;
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
        }
        j.commit()
    }

    /// Puts back each game file `plan` replaces that is missing from the game
    /// folder from its backed-up original. Refuses the plan, before putting
    /// any back, when one of them has none.
    pub(super) fn put_back_missing(&self, plan: &Plan, baseline: &Baseline, objects: &Objects) -> Result<()> {
        let (mut backed_up, mut missing) = (Vec::new(), Vec::new());
        for rel in plan.data.keys() {
            if std::fs::symlink_metadata(self.path(rel)).is_ok() {
                continue;
            }
            match baseline.files.get(rel).filter(|r| objects.has(&r.sha256)) {
                Some(rec) => backed_up.push((rel, rec.sha256.clone())),
                None => missing.push(rel.clone()),
            }
        }
        if !missing.is_empty() {
            return Err(Error::MissingStock {
                files: missing,
                version: crate::game_folder::exe_version::read(&self.path(crate::GAME_EXE)),
            });
        }
        for (rel, sha) in backed_up {
            self.restore_original(&sha, &self.path(rel))?;
            (self.out)(&format!(
                "{rel} was missing from the game folder; put back its backed-up original"
            ));
        }
        Ok(())
    }

    /// Checks each game file `plan` replaces against its recorded original,
    /// recording it when it has none and re-recording one Steam proves the
    /// record stale. Returns the files in the way and the ones with no
    /// original that may be recorded; stops at a file that is not Steam's.
    pub(super) fn check_replacements(
        &self,
        plan: &Plan,
        hashes: &mut Hashes,
        baseline: &mut Baseline,
        objects: &Objects,
        placed: &Placed,
        after: &BTreeMap<&str, String>,
    ) -> Result<(Vec<Conflict>, Vec<String>)> {
        let mut conflicts = Vec::new();
        let mut unproven = Vec::new();
        for rel in plan.data.keys() {
            let path = self.path(rel);
            if !baseline.files.contains_key(rel)
                && let Some(left) = placed.get(rel).filter(|p| !p.replaced)
                && hashes.sha256_opt(&path)?.as_deref() == Some(left.sha256.as_str())
            {
                conflicts.push(Conflict {
                    path: rel.clone(),
                    sha256: Some(left.sha256.clone()),
                    owned: true,
                    replaced: false,
                });
                continue;
            }
            if !baseline.files.contains_key(rel) && !self.may_record(baseline, objects, placed, rel)? {
                unproven.push(rel.clone());
                continue;
            }
            let rec = baseline.record(objects, &self.game.dir, rel, self.depot)?.clone();
            let now = hashes.sha256(&path)?;
            if now != rec.sha256 {
                let owned = placed.wrote(rel, &now) || after[rel.as_str()] == now;
                if !owned {
                    match self.depot.stock(rel, &path)? {
                        Some(true) => {
                            let size = std::fs::metadata(&path).map_err(io_err(&path))?.len();
                            let sha256 = objects.put_file(&path)?;
                            baseline.files.insert(
                                rel.clone(),
                                FileRecord {
                                    size,
                                    sha256,
                                    source: crate::game_folder::baseline::Source::Depot,
                                },
                            );
                            (self.out)(&format!(
                                "{rel} matches Steam's copy; its recorded original was out of date and has been replaced"
                            ));
                            continue;
                        }
                        Some(false) => {
                            (self.out)(&format!(
                                "{rel} is not Steam's copy and swofty did not write it: {}",
                                describe_file(&path, &now)
                            ));
                            return Err(Error::NotStock(vec![rel.clone()]));
                        }
                        None => (self.out)(&format!(
                            "{rel} differs from its recorded original and cannot be checked against Steam's list: {}",
                            describe_file(&path, &now)
                        )),
                    }
                }
                conflicts.push(Conflict {
                    owned,
                    path: rel.clone(),
                    sha256: Some(now),
                    replaced: true,
                });
            }
        }
        Ok((conflicts, unproven))
    }

    /// Refuses the plan when a file it replaces has no original that may be
    /// recorded, after saving the baseline so a rebuild tells the player to
    /// verify the game files.
    pub(super) fn check_unproven(&self, baseline: &mut Baseline, unproven: &[String]) -> Result<()> {
        if unproven.is_empty() {
            return Ok(());
        }
        let rebuilt = baseline.rebuild.is_some();
        if let Some(r) = baseline.rebuild.as_mut() {
            r.told_to_verify = true;
        }
        baseline.save(self.data)?;
        let first = &unproven[0];
        let more = match unproven.len() {
            1 => String::new(),
            n => format!(" (and {} more)", n - 1),
        };
        Err(Error::Other(if rebuilt {
            format!(
                "{}. {first}{more} could not be checked against a backed-up original; nothing in the game folder was changed",
                crate::game_folder::baseline::DAMAGED
            )
        } else {
            format!(
                "{}. {first}{more} still holds a file swofty wrote, and no original is on record for it; nothing in the game folder was changed",
                crate::game_folder::baseline::DAMAGED
            )
        }))
    }

    /// The files `plan` adds that are already in the game folder, each owned
    /// when it holds what the launcher wrote or is about to write there.
    pub(super) fn check_new_files(
        &self,
        plan: &Plan,
        hashes: &mut Hashes,
        baseline: &Baseline,
        placed: &Placed,
        after: &BTreeMap<&str, String>,
    ) -> Result<Vec<Conflict>> {
        let mut conflicts = Vec::new();
        for rel in plan.added.keys().chain(plan.loader.keys()) {
            let path = self.path(rel);
            if std::fs::symlink_metadata(&path).is_err() {
                continue;
            }
            let now = hashes.sha256_opt(&path)?;
            let stock = baseline.files.contains_key(rel);
            conflicts.push(Conflict {
                owned: now
                    .as_deref()
                    .is_some_and(|now| placed.wrote(rel, now) || (!stock && after[rel.as_str()] == now)),
                path: rel.clone(),
                sha256: now,
                replaced: false,
            });
        }
        Ok(conflicts)
    }

    /// Opens the journal of `plan`'s install with every step pending, after
    /// recording what it places and the user.reg it edits. Returns the
    /// journal and that user.reg with its text before and after the override.
    pub(super) fn open_journal(
        &self,
        plan: &Plan,
        baseline: &Baseline,
        objects: &Objects,
        placed: &mut Placed,
        after: &BTreeMap<&str, String>,
    ) -> Result<(Journal, Option<RegistryEdit<'a>>)> {
        let mut j = Journal::create(
            self.data,
            &plan.profile,
            &self.game.identity.build_id,
            self.session,
        )?;
        j.game = Some(self.game.dir.clone());
        for rel in plan.data.keys() {
            placed.insert(rel, after[rel.as_str()].clone(), true);
        }
        for rel in plan.added.keys().chain(plan.loader.keys()) {
            placed.insert(rel, after[rel.as_str()].clone(), false);
        }
        let registry = self.registry_edit(plan, placed)?;
        if let Some((_, name, _, _)) = &registry {
            placed.registry = Some(name.clone());
        }
        placed.save(self.data)?;
        let mut steps = self.steps(plan, baseline, after);
        let registry = match registry {
            Some((reg, name, text, out)) => {
                let (before, now) = (objects.put_file(reg)?, sha256_bytes(out.as_bytes()));
                steps.push((Op::RegistryOverride, name, Some(before), Some(now)));
                Some((reg, text, out))
            }
            None => None,
        };
        j.begin_all(steps)?;
        Ok((j, registry))
    }

    /// The user.reg `plan` overrides, its name in the journal, and its text
    /// now and with the override; one that has the override already counts
    /// only when the launcher put it there.
    pub(super) fn registry_edit(
        &self,
        plan: &Plan,
        placed: &Placed,
    ) -> Result<Option<(&'a PathBuf, String, String, String)>> {
        let game = self.game;
        Ok(match &game.user_reg {
            Some(reg) if plan.registry_override => {
                let text = std::fs::read_to_string(reg).map_err(io_err(reg))?;
                let name = reg.display().to_string();
                let ours = placed.registry.as_deref() == Some(name.as_str());
                match add_override(&text) {
                    Some(out) => Some((reg, name, text, out)),
                    None => ours.then(|| (reg, name, text.clone(), text)),
                }
            }
            _ => None,
        })
    }

    /// The journal steps that place `plan`'s files: each replacement, then
    /// each new file after the folders it needs that are not there yet.
    pub(super) fn steps(
        &self,
        plan: &Plan,
        baseline: &Baseline,
        after: &BTreeMap<&str, String>,
    ) -> Vec<Step> {
        let mut steps = Vec::new();
        for rel in plan.data.keys() {
            let before = baseline.files[rel].sha256.clone();
            steps.push((
                Op::ReplaceFile,
                rel.clone(),
                Some(before),
                Some(after[rel.as_str()].clone()),
            ));
        }
        let mut dirs = BTreeSet::new();
        for rel in plan.added.keys().chain(plan.loader.keys()) {
            for dir in rel_parts(rel) {
                if !self.path(&dir).is_dir() && dirs.insert(dir.clone()) {
                    steps.push((Op::CreateDir, dir, None, None));
                }
            }
            steps.push((
                Op::CreateFile,
                rel.clone(),
                None,
                Some(after[rel.as_str()].clone()),
            ));
        }
        steps
    }

    /// Carries out each pending step of `j` in order, linking the files it
    /// can and copying the rest, and marks each done.
    pub(super) fn copy_entries(
        &self,
        j: &mut Journal,
        sources: &BTreeMap<&str, &PathBuf>,
        stash: bool,
        hashes: &mut Hashes,
        registry: Option<&RegistryEdit<'a>>,
    ) -> Result<Copied> {
        let (mut n, mut told) = (0, std::time::Instant::now());
        let built = crate::store::overlay_root(self.data);
        let LinkCache {
            via,
            synced: cached,
            failure: cache_failure,
        } = if self.copy_only {
            LinkCache::default()
        } else {
            self.link_cache(&built, &j.entries, sources)
        };
        let mut linker = Linker {
            via,
            built,
            on: !self.copy_only,
            from_cache: 0,
            failure: None,
        };
        let (mut linked, mut placed) = (0, 0);
        for i in 0..j.entries.len() {
            self.step(&mut n)?;
            let e = &j.entries[i];
            let put = self.put_entry(e, sources, &mut linker, stash, hashes, registry)?;
            match put {
                Put::Refused => continue,
                Put::Linked => linked += 1,
                Put::Copied | Put::Made => {}
            }
            if matches!(put, Put::Linked | Put::Copied) {
                let path = self.path(&e.path);
                hashes.wrote(&path, e.after.as_deref().expect("a placed file has after"));
                placed += 1;
            }
            j.done(i)?;
            (self.progress)(placed, sources.len());
            if told.elapsed() >= PROGRESS_EVERY {
                (self.out)(&format!(
                    "installing: {placed} of {} file(s) copied",
                    sources.len()
                ));
                told = std::time::Instant::now();
            }
        }
        Ok(Copied {
            linked,
            from_cache: linker.from_cache,
            placed,
            link_failure: linker.failure,
            cached,
            cache_failure,
        })
    }

    /// Carries out one journal step. A replaced original moved aside goes
    /// back at once when its replacement can't be copied.
    pub(super) fn put_entry(
        &self,
        e: &Entry,
        sources: &BTreeMap<&str, &PathBuf>,
        linker: &mut Linker,
        stash: bool,
        hashes: &mut Hashes,
        registry: Option<&RegistryEdit<'a>>,
    ) -> Result<Put> {
        let path = &self.path(&e.path);
        Ok(match e.op {
            Op::ReplaceFile => {
                let src = sources[e.path.as_str()];
                let before = e.before.as_deref().expect("replace-file has before");
                let stashed = stash && self.stash(path, before, hashes);
                if linker.link(&e.path, src, path, true) {
                    hashes.wrote(src, e.after.as_deref().expect("a placed file has after"));
                    Put::Linked
                } else if let Err(err) = copy_replacing(src, path) {
                    if stashed {
                        self.unstash(before, path, hashes);
                    }
                    return Err(crate::diagnostics::quarantine::copy_error(&e.path, path, err));
                } else {
                    crate::clear_readonly(path);
                    Put::Copied
                }
            }
            Op::CreateDir => {
                std::fs::create_dir(path).map_err(io_err(path))?;
                Put::Made
            }
            // Windows refused the file as a threat. The entry stays pending, so
            // the rollback clears whatever was left, and the check before the
            // game starts finds the file missing and refuses the launch with
            // the antivirus report instead of a bare copy error here.
            Op::CreateFile => {
                let src = sources[e.path.as_str()];
                if linker.link(&e.path, src, path, false) {
                    hashes.wrote(src, e.after.as_deref().expect("a placed file has after"));
                    Put::Linked
                } else {
                    match copy_replacing(src, path) {
                        Err(err) if crate::diagnostics::quarantine::is_virus_error(&err) => Put::Refused,
                        r => {
                            r.map_err(io_err(path))?;
                            crate::clear_readonly(path);
                            Put::Copied
                        }
                    }
                }
            }
            Op::RegistryOverride => {
                if let Some((reg, text, out)) = registry
                    && out != text
                {
                    std::fs::write(reg, out).map_err(io_err(*reg))?;
                }
                Put::Made
            }
        })
    }

    /// Loads the storage of each addon `sources` places for the plan's
    /// profile, and puts the files it kept there back in the game folder,
    /// except the ones the plan places.
    pub(super) fn put_back_storage(&self, plan: &Plan, sources: &BTreeMap<&str, &PathBuf>) -> Result<()> {
        let planned: BTreeSet<String> = sources.keys().map(|rel| placed::key(rel)).collect();
        for id in addon_storage::ids(sources.keys().copied()) {
            addon_storage::load_profile(self.data, &id, &plan.profile)?;
            let moved = addon_storage::put_back(self.data, &self.game.dir, &id, &planned)?;
            if !moved.is_empty() {
                (self.out)(&format!(
                    "storage: put back {} file(s) {id} wrote, from {}",
                    moved.len(),
                    addon_storage::addon_dir(self.data, &id).display()
                ));
            }
        }
        Ok(())
    }

    /// Brings the link cache on the game's drive up to date for the files
    /// that can't be linked straight from the build output: all of them when
    /// the build output is on another drive than the game folder, else the
    /// ones from outside it, such as loose addon files. Returns where each is
    /// in the cache, what the sync did, and why there is no cache when it
    /// could not be made.
    pub(super) fn link_cache(
        &self,
        built: &Path,
        entries: &[crate::game_folder::journal::Entry],
        sources: &BTreeMap<&str, &PathBuf>,
    ) -> LinkCache {
        let apart = crate::cache::link::apart((self.volume)(built), (self.volume)(&self.game.dir));
        let srcs: Vec<crate::cache::link::Wanted> = entries
            .iter()
            .filter(|e| matches!(e.op, Op::ReplaceFile | Op::CreateFile))
            .map(|e| {
                (
                    e.path.as_str(),
                    sources[e.path.as_str()].as_path(),
                    e.after.as_deref(),
                )
            })
            .filter(|(_, s, _)| apart == Some(true) || !s.starts_with(built))
            .collect();
        if srcs.is_empty() {
            if crate::cache::link::dir(&self.game.dir).exists() {
                crate::cache::link::remove(&self.game.dir);
            }
            return LinkCache::default();
        }
        crate::now_doing("copying the mods to the game drive");
        let mut told = std::time::Instant::now();
        let synced = crate::cache::link::sync(&self.game.dir, &srcs, self.free_space, &mut |done, total| {
            (self.progress)(done, total);
            if told.elapsed() >= PROGRESS_EVERY {
                (self.out)(&format!(
                    "installing: {done} of {total} file(s) copied into the link cache"
                ));
                told = std::time::Instant::now();
            }
        });
        match synced {
            Ok(s) => LinkCache {
                via: s.files,
                synced: Some((s.copied, s.reused)),
                failure: None,
            },
            Err(why) => {
                (self.out)(&format!(
                    "installing: link cache unavailable ({why}); copying instead"
                ));
                LinkCache {
                    failure: Some(why),
                    ..LinkCache::default()
                }
            }
        }
    }
}

/// One journal step: what it does, to which game path, and the file's hash
/// before and after.
pub(super) type Step = (Op, String, Option<String>, Option<String>);

/// The user.reg an install overrides, with its text before and after.
pub(super) type RegistryEdit<'a> = (&'a PathBuf, String, String);

/// What carrying out a journal step did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Put {
    /// The file is a link to its build output or its copy in the link cache.
    Linked,
    /// The file is a copy.
    Copied,
    /// A folder or the prefix override.
    Made,
    /// Windows refused the file as a threat; the step stays pending.
    Refused,
}

/// Links placed files from the link cache or the build output until a link
/// fails, and counts the ones from the cache.
pub(super) struct Linker {
    pub(super) via: BTreeMap<String, PathBuf>,
    pub(super) built: PathBuf,
    pub(super) on: bool,
    pub(super) from_cache: usize,
    pub(super) failure: Option<String>,
}

impl Linker {
    /// Whether `path` is now a link to the cached copy of `rel`, or else to
    /// `src` in the build output. False when neither may be linked, or when
    /// the link failed, which ends linking.
    pub(super) fn link(&mut self, rel: &str, src: &Path, path: &Path, replacing: bool) -> bool {
        let cache = self.via.get(rel);
        if !self.on || (cache.is_none() && !src.starts_with(&self.built)) {
            return false;
        }
        let from_cache = cache.is_some();
        let src = cache.map_or(src, |p| p.as_path());
        let made = if replacing {
            link_replacing(src, path)
        } else {
            std::fs::hard_link(src, path)
        };
        match made {
            Ok(()) => {
                self.from_cache += usize::from(from_cache);
                true
            }
            Err(err) => {
                self.failure = Some(err.to_string());
                self.on = false;
                false
            }
        }
    }
}

/// What copying an install's files did: the files linked, of them the ones
/// from the link cache, the files placed in all, why linking stopped, and
/// what the link cache sync copied and reused or why there is no cache.
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Copied {
    pub(super) linked: usize,
    pub(super) from_cache: usize,
    pub(super) placed: usize,
    pub(super) link_failure: Option<String>,
    pub(super) cached: Option<(usize, usize)>,
    pub(super) cache_failure: Option<String>,
}

/// The line an install logs on how it placed its files.
pub(super) fn copy_report(c: &Copied, copy_only: bool) -> String {
    let (linked, copied) = (c.linked, c.placed - c.linked);
    let mode = match (&c.link_failure, copy_only) {
        (_, true) => "copy (SWOFTY_INSTALL_COPY=1)".to_owned(),
        (Some(why), _) if c.cached.is_some() => format!("copy (link cache link failed: {why})"),
        (None, _) if c.cached.is_some() => {
            let (made, kept) = c.cached.unwrap_or_default();
            return format!(
                "installing: {linked} linked ({} from the link cache: {made} copied into it, {kept} reused), {copied} copied",
                c.from_cache
            );
        }
        (None, _) if linked == 0 && c.cache_failure.is_some() => {
            format!(
                "copy (link cache unavailable: {})",
                c.cache_failure.as_deref().unwrap_or_default()
            )
        }
        (Some(why), _) => format!("copy (hard link failed: {why})"),
        (None, _) if linked == 0 => "copy (no build output to link)".to_owned(),
        (None, _) => "hard links".to_owned(),
    };
    format!("installing: {mode}; {linked} file(s) linked, {copied} copied")
}

/// Refuses a plan that would replace an upscaler drop-in the player put in
/// the game folder.
pub(super) fn refuse_drop_ins(plan: &Plan) -> Result<()> {
    match plan
        .data
        .keys()
        .chain(plan.added.keys())
        .chain(plan.loader.keys())
        .find(|rel| crate::game_folder::stock::is_upscaler_drop_in(rel))
    {
        Some(rel) => Err(Error::Resolve(format!(
            "{rel} is an upscaler drop-in file and stays as the player put it; no addon or profile can replace it"
        ))),
        None => Ok(()),
    }
}

/// The hash of each file `plan` places, by game path.
pub(super) fn hashed<'p>(plan: &'p Plan, hashes: &mut Hashes) -> Result<BTreeMap<&'p str, String>> {
    let mut after = BTreeMap::new();
    for (rel, src) in plan.data.iter().chain(&plan.added).chain(&plan.loader) {
        after.insert(rel.as_str(), hashes.sha256(src)?);
    }
    Ok(after)
}

/// The source of each file `plan` places, by game path.
pub(super) fn sources(plan: &Plan) -> BTreeMap<&str, &PathBuf> {
    plan.data
        .iter()
        .chain(&plan.added)
        .chain(&plan.loader)
        .map(|(rel, src)| (rel.as_str(), src))
        .collect()
}
