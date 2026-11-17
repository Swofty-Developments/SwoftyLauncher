use std::path::Path;

use swofty_formats::erp::Entry;

use super::checks::is_handling;
use super::{Archive, ErpBuild, Slot, fail, served_at_runtime};
use crate::content::runtime_assets::{foreign_argument, layout_change};
use crate::{Error, Result};

pub(super) const HANDLING_WHY: &str =
    "a car handling file is read from the game file, as a hand copy puts it";

#[derive(Debug, Clone, PartialEq)]
pub struct Resized {
    pub erp: String,
    pub resources: Vec<(String, String)>,
}

/// A hash of everything that decides an archive's bytes: the build pipeline,
/// the source archive (path, size and modified time) and every slot, stock by
/// index and owned by all of its fields and fragment bytes.
fn archive_key(rel: &str, a: &Archive) -> Option<String> {
    use sha2::{Digest, Sha256};
    let meta = std::fs::metadata(&a.file.path).ok()?;
    let mut h = Sha256::new();
    let mut field = |b: &[u8]| {
        h.update((b.len() as u64).to_le_bytes());
        h.update(b);
    };
    field(env!("SWOFTY_PIPELINE").as_bytes());
    field(rel.as_bytes());
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
                    field(&f.packed.to_le_bytes());
                    field(&f.raw);
                }
                field(b"end");
            }
        }
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Write `archive` to `out`, or link it from `cache` when an earlier build
/// wrote the same bytes; a newly written one is linked into the cache. True
/// when it came from the cache. Without hard links nothing is cached.
fn write_archive(out: &Path, rel: &str, archive: Archive, cache: Option<&Path>) -> Result<bool> {
    let path = out.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
    }
    let cached = cache.and_then(|dir| {
        let key = archive_key(rel, &archive)?;
        Some(dir.join(&key[..2]).join(format!("{key}.erp")))
    });
    if let Some(c) = cached.as_ref().filter(|c| c.is_file())
        && std::fs::hard_link(c, &path).is_ok()
    {
        if let Ok(f) = std::fs::File::options().write(true).open(c) {
            f.set_modified(std::time::SystemTime::now()).ok();
        }
        return Ok(true);
    }
    let Archive { file, slots } = archive;
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

impl ErpBuild<'_> {
    // The in-game loader reads a replaced resource with the stock entry's
    // layout, so an archive whose changes alter a stock entry's type, argument
    // or fragment sizes is rewritten whole instead, and leaves the runtime set.
    pub fn resized(&self) -> Vec<Resized> {
        let mut found = Vec::new();
        for (rel, a) in &self.erps {
            let stock = &a.file.erp;
            let resources: Vec<(String, String)> = a
                .slots
                .iter()
                .filter_map(|s| match s {
                    Slot::Owned(r) if !foreign_argument(r, stock) => layout_change(r, stock)
                        .or_else(|| is_handling(r).then(|| HANDLING_WHY.to_owned()))
                        .map(|why| (r.name.clone(), why)),
                    _ => None,
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
    // reaches. Those are written whole: the stock archive with every addon's resources.
    pub fn take_outside(&mut self, out: &Path) -> Result<Vec<String>> {
        let outside: Vec<String> = self
            .erps
            .iter()
            .filter(|(rel, a)| !served_at_runtime(rel) && !a.untouched())
            .map(|(rel, _)| rel.clone())
            .collect();
        for rel in &outside {
            let archive = self.erps.remove(rel).expect("listed above");
            write_archive(out, rel, archive, None)?;
        }
        Ok(outside)
    }

    pub fn take_resized(&mut self, out: &Path) -> Result<Vec<Resized>> {
        let found = self.resized();
        for r in &found {
            let archive = self.erps.remove(&r.erp).expect("found above");
            write_archive(out, &r.erp, archive, None)?;
        }
        Ok(found)
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
        let mut written = Vec::new();
        let mut reused = 0;
        for (rel, archive) in self.erps.into_iter().filter(|(_, a)| !a.untouched()) {
            cancel()?;
            crate::now_doing(format!("writing {rel}"));
            reused += usize::from(write_archive(out, &rel, archive, cache)?);
            written.push(rel);
        }
        Ok((written, reused))
    }
}
