use std::path::{Path, PathBuf};

use swofty_formats::erp::{Packer, Resource};

use super::checks::is_handling;
use super::{Archive, ErpBuild, fail, pack, served_at_runtime};
use crate::cache::archive_entry::ArchiveEntry;
use crate::content::runtime_assets::{foreign_argument, layout_change};
use crate::{Error, Result};

pub(super) const HANDLING_WHY: &str =
    "a car handling file is read from the game file, as a hand copy puts it";

#[derive(Debug, Clone, PartialEq)]
pub struct Resized {
    pub erp: String,
    pub resources: Vec<(String, String)>,
}

/// The archives a runtime-route build writes whole instead of the in-game loader patching them.
#[derive(Debug, Default)]
pub struct BuiltIn {
    /// The archives outside the asset groups, which the in-game loader never patches.
    pub outside: Vec<String>,
    /// The archives whose changes alter a stock entry's layout.
    pub resized: Vec<Resized>,
    /// How many of them came from the cache.
    pub reused: usize,
}

/// The write of one archive: the archive as the ops left it, the file it goes to and the cache of
/// archives earlier builds wrote. A write owns all it reads and writes, so the writes of different
/// archives share nothing.
struct ArchiveWrite {
    rel: String,
    archive: Archive,
    path: PathBuf,
    cache: Option<PathBuf>,
}

impl ArchiveWrite {
    fn new(out: &Path, rel: String, archive: Archive, cache: Option<&Path>) -> Self {
        Self {
            path: out.join(&rel),
            rel,
            archive,
            cache: cache.map(Path::to_path_buf),
        }
    }

    fn entry(&self) -> Option<ArchiveEntry> {
        self.archive.cache_entry(self.cache.as_deref()?)
    }

    /// Links the archive from the cache `entry` an earlier build wrote and marks the entry used, leaving
    /// the archive's write time as it was; true when it did. Without hard links nothing is cached.
    fn link(&self, entry: Option<&ArchiveEntry>) -> Result<bool> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
        }
        Ok(entry.is_some_and(|e| e.link_to(&self.path)))
    }

    /// Writes the archive, its fragments compressed, and keeps it in the cache at `entry`.
    fn write(self, entry: Option<ArchiveEntry>) -> Result<()> {
        let Self {
            rel, archive, path, ..
        } = self;
        archive.write_to(&path).map_err(|e| match e {
            swofty_formats::Error::IoAt { .. } => Error::Formats(e),
            e => fail(format!("{rel}: {e}")),
        })?;
        if let Some(e) = entry {
            e.keep(&path);
        }
        Ok(())
    }
}

/// Runs `writes`: links from the cache each archive an earlier build wrote the same, compresses the
/// pending fragments of the others and of `more` together, one batch per level across all of them,
/// and writes those archives. For each write in order, its archive and whether it came from the cache.
/// `cancel` runs before each archive.
fn run_all<'r>(
    writes: Vec<ArchiveWrite>,
    packer: &Packer,
    more: impl IntoIterator<Item = &'r mut Resource>,
    cancel: &dyn Fn() -> Result<()>,
) -> Result<Vec<(String, bool)>> {
    let mut runs = Vec::with_capacity(writes.len());
    for write in writes {
        cancel()?;
        let entry = write.entry();
        let linked = write.link(entry.as_ref())?;
        runs.push((write, entry, linked));
    }
    let mut owned: Vec<&mut Resource> = runs
        .iter_mut()
        .filter(|(_, _, linked)| !linked)
        .flat_map(|(write, _, _)| write.archive.written_mut())
        .collect();
    for r in more {
        owned.push(r);
    }
    crate::now_doing("compressing the changed fragments");
    pack::settle(packer, owned)?;
    let mut done = Vec::with_capacity(runs.len());
    for (write, entry, linked) in runs {
        let rel = write.rel.clone();
        if !linked {
            cancel()?;
            crate::now_doing(format!("writing {rel}"));
            write.write(entry)?;
        }
        done.push((rel, linked));
    }
    Ok(done)
}

fn reused(done: &[(String, bool)]) -> usize {
    done.iter().filter(|(_, linked)| *linked).count()
}

impl ErpBuild<'_> {
    /// Compresses every written fragment still pending, one batch per level across all the archives, for
    /// a caller that copies the resources out instead of writing the archives; how many it compressed.
    pub fn settle(&mut self) -> Result<usize> {
        let packer = self.packer.clone();
        pack::settle(&packer, self.erps.values_mut().flat_map(|a| a.written_mut()))
    }

    // The in-game loader reads a replaced resource with the stock entry's
    // layout, so an archive whose changes alter a stock entry's type, argument
    // or fragment sizes is rewritten whole instead, and leaves the runtime set.
    pub fn resized(&self) -> Vec<Resized> {
        let mut found = Vec::new();
        for (rel, a) in &self.erps {
            let stock = &a.file.erp;
            let resources: Vec<(String, String)> = a
                .written()
                .filter(|r| !foreign_argument(r, stock))
                .filter_map(|r| {
                    layout_change(r, stock)
                        .or_else(|| is_handling(r).then(|| HANDLING_WHY.to_owned()))
                        .map(|why| (r.name.clone(), why))
                })
                .collect();
            if !resources.is_empty() {
                found.push(Resized {
                    erp: rel.clone(),
                    resources,
                });
            }
        }
        found
    }

    // The in-game loader patches only archives the asset groups list, through the
    // manifest's name map; the engine opens any other archive, such as the shader
    // packages Simplified Studio packs mods into, through a hashed path an overlay never
    // reaches. Those are written whole: the stock archive with every addon's resources,
    // linked from `cache` when an earlier build wrote the same. The archives written and
    // how many came from the cache.
    pub fn take_outside(&mut self, out: &Path, cache: Option<&Path>) -> Result<(Vec<String>, usize)> {
        let outside = self.outside();
        let writes = outside.iter().map(|rel| self.write_of(out, rel, cache)).collect();
        let done = run_all(writes, &self.packer, std::iter::empty(), &|| Ok(()))?;
        Ok((outside, reused(&done)))
    }

    fn outside(&self) -> Vec<String> {
        self.erps
            .iter()
            .filter(|(rel, a)| !served_at_runtime(rel) && !a.untouched())
            .map(|(rel, _)| rel.clone())
            .collect()
    }

    fn write_of(&mut self, out: &Path, rel: &str, cache: Option<&Path>) -> ArchiveWrite {
        let archive = self.erps.remove(rel).expect("an archive of the build");
        ArchiveWrite::new(out, rel.to_owned(), archive, cache)
    }

    /// Writes whole the archives [`Self::resized`] finds, linking from `cache` the ones an earlier
    /// build wrote the same; those archives and how many came from the cache.
    pub fn take_resized(&mut self, out: &Path, cache: Option<&Path>) -> Result<(Vec<Resized>, usize)> {
        let found = self.resized();
        let writes = found.iter().map(|r| self.write_of(out, &r.erp, cache)).collect();
        let done = run_all(writes, &self.packer, std::iter::empty(), &|| Ok(()))?;
        Ok((found, reused(&done)))
    }

    /// Takes out and writes whole the archives [`Self::take_outside`] and then [`Self::take_resized`]
    /// take, linking from `cache` the ones an earlier build wrote the same, and compresses the pending
    /// fragments of the archives left, which the runtime overlay copies out, in the same batches.
    pub fn take_built_in(
        &mut self,
        out: &Path,
        cache: Option<&Path>,
        cancel: &dyn Fn() -> Result<()>,
    ) -> Result<BuiltIn> {
        let outside = self.outside();
        let mut writes: Vec<ArchiveWrite> =
            outside.iter().map(|rel| self.write_of(out, rel, cache)).collect();
        let resized = self.resized();
        writes.extend(resized.iter().map(|r| self.write_of(out, &r.erp, cache)));
        let packer = self.packer.clone();
        let left = self.erps.values_mut().flat_map(|a| a.written_mut());
        let done = run_all(writes, &packer, left, cancel)?;
        Ok(BuiltIn {
            outside,
            resized,
            reused: reused(&done),
        })
    }

    pub fn write(self, out: &Path) -> Result<Vec<String>> {
        Ok(self.write_cached(out, None, &|| Ok(()))?.0)
    }

    /// Write the changed archives to `out`, linking from `cache` the ones an
    /// earlier build wrote the same; the archives written and how many came
    /// from the cache. `cancel` runs before each archive.
    pub fn write_cached(
        self,
        out: &Path,
        cache: Option<&Path>,
        cancel: &dyn Fn() -> Result<()>,
    ) -> Result<(Vec<String>, usize)> {
        let writes: Vec<ArchiveWrite> = self
            .erps
            .into_iter()
            .filter(|(_, a)| !a.untouched())
            .map(|(rel, archive)| ArchiveWrite::new(out, rel, archive, cache))
            .collect();
        let done = run_all(writes, &self.packer, std::iter::empty(), cancel)?;
        let reused = reused(&done);
        Ok((done.into_iter().map(|(rel, _)| rel).collect(), reused))
    }
}
