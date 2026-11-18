use std::path::{Path, PathBuf};

use swofty_formats::erp::{Entry, Packer};

use super::checks::is_handling;
use super::{Archive, ErpBuild, Slot, fail, pack, served_at_runtime};
use crate::content::runtime_assets::{foreign_argument, layout_change};
use crate::{Error, Result};

/// The version of the cache key's layout, raised whenever what [`ArchiveWrite::key`] hashes changes, so
/// an archive cached under an older layout is never taken for one written now. 2: a fragment hashes as
/// stored or as pending.
const KEY_VERSION: u32 = 2;

pub(super) const HANDLING_WHY: &str =
    "a car handling file is read from the game file, as a hand copy puts it";

#[derive(Debug, Clone, PartialEq)]
pub struct Resized {
    pub erp: String,
    pub resources: Vec<(String, String)>,
}

/// The write of one archive: the archive as the ops left it, the file it goes to, the cache of
/// archives earlier builds wrote and the packer that compresses its pending fragments. A write owns
/// all it reads and writes, so the writes of different archives share nothing.
struct ArchiveWrite {
    rel: String,
    archive: Archive,
    path: PathBuf,
    cache: Option<PathBuf>,
    packer: Packer,
}

impl ArchiveWrite {
    fn new(out: &Path, rel: String, archive: Archive, cache: Option<&Path>, packer: &Packer) -> Self {
        Self {
            path: out.join(&rel),
            rel,
            archive,
            cache: cache.map(Path::to_path_buf),
            packer: packer.clone(),
        }
    }

    /// A hash of everything that decides the archive's bytes: the key's layout, the build pipeline,
    /// the source archive (path, size and modified time) and every slot, stock by index and owned by
    /// all of its fields and fragments: stored bytes, or the data and level of a pending compression.
    fn key(&self) -> Option<String> {
        use sha2::{Digest, Sha256};
        let a = &self.archive;
        let meta = std::fs::metadata(&a.file.path).ok()?;
        let mut h = Sha256::new();
        let mut field = |b: &[u8]| {
            h.update((b.len() as u64).to_le_bytes());
            h.update(b);
        };
        field(&KEY_VERSION.to_le_bytes());
        field(env!("SWOFTY_PIPELINE").as_bytes());
        field(self.rel.as_bytes());
        field(a.file.path.to_string_lossy().as_bytes());
        field(&meta.len().to_le_bytes());
        field(format!("{:?}", meta.modified().ok()?).as_bytes());
        field(&a.file.erp.header);
        field(&a.file.erp.version.to_le_bytes());
        for slot in &a.slots {
            match slot {
                Slot::Stock(i) => {
                    field(b"stock");
                    field(&(*i as u64).to_le_bytes());
                }
                Slot::Owned(r) => {
                    field(b"owned");
                    field(r.name.as_bytes());
                    field(&r.kind);
                    field(&r.unk);
                    field(&r.hash);
                    for f in &r.fragments {
                        field(&f.name);
                        field(&f.offset.to_le_bytes());
                        field(&f.size.to_le_bytes());
                        field(&f.flags.to_le_bytes());
                        field(&[f.compression]);
                        match f.pending() {
                            Some((data, level)) => {
                                field(b"pending");
                                field(&level.to_le_bytes());
                                field(data);
                            }
                            None => {
                                field(b"stored");
                                field(&f.packed().unwrap_or_default().to_le_bytes());
                                field(f.raw().unwrap_or_default());
                            }
                        }
                    }
                    field(b"end");
                }
            }
        }
        Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Writes the archive, or links it from the cache when an earlier build
    /// wrote the same bytes; a newly written one is linked into the cache. True
    /// when it came from the cache. Without hard links nothing is cached. The
    /// fragments still pending are compressed first, one batch per level.
    fn run(self) -> Result<bool> {
        let path = &self.path;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
        }
        let cached = self.cache.as_deref().and_then(|dir| {
            let key = self.key()?;
            Some(dir.join(&key[..2]).join(format!("{key}.erp")))
        });
        if let Some(c) = cached.as_ref().filter(|c| c.is_file())
            && std::fs::hard_link(c, path).is_ok()
        {
            if let Ok(f) = std::fs::File::options().write(true).open(c) {
                f.set_modified(std::time::SystemTime::now()).ok();
            }
            return Ok(true);
        }
        let Self {
            rel,
            archive,
            path,
            packer,
            ..
        } = self;
        let Archive { file, mut slots } = archive;
        pack::settle(
            &packer,
            slots.iter_mut().filter_map(|s| match s {
                Slot::Owned(r) => Some(r),
                Slot::Stock(_) => None,
            }),
        )?;
        let entries = slots
            .into_iter()
            .map(|s| match s {
                Slot::Stock(i) => Entry::Copy(&file.erp.resources[i]),
                Slot::Owned(r) => Entry::Owned(r),
            })
            .collect();
        file.write(entries, &path).map_err(|e| match e {
            swofty_formats::Error::IoAt { .. } => Error::Formats(e),
            e => fail(format!("{rel}: {e}")),
        })?;
        if let Some(c) = cached {
            if let Some(dir) = c.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            std::fs::hard_link(&path, &c).ok();
        }
        Ok(false)
    }
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
        let outside: Vec<String> = self
            .erps
            .iter()
            .filter(|(rel, a)| !served_at_runtime(rel) && !a.untouched())
            .map(|(rel, _)| rel.clone())
            .collect();
        let mut reused = 0;
        for rel in &outside {
            let archive = self.erps.remove(rel).expect("listed above");
            reused += usize::from(ArchiveWrite::new(out, rel.clone(), archive, cache, &self.packer).run()?);
        }
        Ok((outside, reused))
    }

    /// Writes whole the archives [`Self::resized`] finds, linking from `cache` the ones an earlier
    /// build wrote the same; those archives and how many came from the cache.
    pub fn take_resized(&mut self, out: &Path, cache: Option<&Path>) -> Result<(Vec<Resized>, usize)> {
        let found = self.resized();
        let mut reused = 0;
        for r in &found {
            let archive = self.erps.remove(&r.erp).expect("found above");
            reused += usize::from(ArchiveWrite::new(out, r.erp.clone(), archive, cache, &self.packer).run()?);
        }
        Ok((found, reused))
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
            .map(|(rel, archive)| ArchiveWrite::new(out, rel, archive, cache, &self.packer))
            .collect();
        let mut written = Vec::new();
        let mut reused = 0;
        for write in writes {
            cancel()?;
            crate::now_doing(format!("writing {}", write.rel));
            let rel = write.rel.clone();
            reused += usize::from(write.run()?);
            written.push(rel);
        }
        Ok((written, reused))
    }
}
