//! Carrying out an install's journal: each file linked from the link cache or the build output, or
//! copied, the folders and the prefix override, and the line that says how.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::files::{copy_replacing, link_replacing};
use super::record::RegistryEdit;
use super::{Installer, Plan};
use crate::game_folder::baseline::Hashes;
use crate::game_folder::journal::{Entry, Journal, Op};
use crate::game_folder::placed;
use crate::{Error, Result, addons::storage as addon_storage, io_err};

pub(super) const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

/// Where each placed file is in the link cache by game path, how many files
/// the sync copied and reused, and why there is no cache when it failed.
#[derive(Default)]
pub(super) struct LinkCache {
    pub(super) via: BTreeMap<String, PathBuf>,
    pub(super) synced: Option<(usize, usize)>,
    pub(super) failure: Option<String>,
}

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

/// Whether the file from `src` is placed from the link cache: every file when the build output in `built` is
/// on another drive than the game folder (`apart`), else the ones from outside it.
pub(super) fn through_cache(apart: Option<bool>, built: &Path, src: &Path) -> bool {
    apart == Some(true) || !src.starts_with(built)
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

impl<'a> Installer<'a> {
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
                    return Err(self.copy_failed(&e.path, src, path, err));
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
                        Err(err) => {
                            return Err(self
                                .source_gone(&e.path, src, path, &err)
                                .unwrap_or_else(|| io_err(path)(err)));
                        }
                        Ok(()) => {
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

    fn copy_failed(&self, rel: &str, src: &Path, path: &Path, err: std::io::Error) -> crate::Error {
        self.source_gone(rel, src, path, &err)
            .unwrap_or_else(|| crate::diagnostics::quarantine::copy_error(rel, path, err))
    }

    fn source_gone(&self, rel: &str, src: &Path, path: &Path, err: &std::io::Error) -> Option<crate::Error> {
        if err.kind() != std::io::ErrorKind::NotFound || src.exists() {
            return None;
        }
        (self.out)(&format!(
            "install: {rel}: source {} is gone, destination {}, route copy",
            src.display(),
            path.display()
        ));
        Some(crate::Error::Other(format!(
            "{} was built by the launcher for {rel} and disappeared before it could be put in the game folder; \
             an antivirus most likely removed it. Allow the launcher's data folder in your antivirus (see \
             \"If the mods don't load\" in the Windows guide), then press Play again. The game folder is unchanged.",
            src.display()
        )))
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
            .filter(|(_, s, _)| through_cache(apart, built, s))
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
