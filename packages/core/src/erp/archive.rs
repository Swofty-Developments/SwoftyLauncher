use std::borrow::Cow;
use std::path::Path;

use swofty_formats::erp::{Entry, ErpFile, Resource};

use super::fail;
use crate::cache::archive_entry::ArchiveEntry;
use crate::{Error, Result};

/// The version of the cache key's layout, raised whenever what [`Archive::key`] hashes changes, so an
/// archive cached under an older layout is never taken for one written now. 2: a fragment hashes as stored
/// or as pending. 3: the key leaves out where the archive is written, which its bytes do not depend on.
const KEY_VERSION: u32 = 3;

pub(crate) enum Slot {
    Stock(usize),
    Owned(Resource),
}

pub(crate) struct Archive {
    pub(super) file: ErpFile,
    pub(super) slots: Vec<Slot>,
}

impl Archive {
    pub(crate) fn new(file: ErpFile, slots: Vec<Slot>) -> Self {
        Self { file, slots }
    }

    /// A hash of everything that decides the archive's bytes: the key's layout, the build pipeline, the
    /// source archive (path, size and modified time) and every slot, stock by index and owned by all of its
    /// fields and fragments: stored bytes, or the data and level of a pending compression.
    pub(crate) fn key(&self) -> Option<String> {
        use sha2::{Digest, Sha256};
        let meta = std::fs::metadata(&self.file.path).ok()?;
        let mut h = Sha256::new();
        let mut field = |b: &[u8]| {
            h.update((b.len() as u64).to_le_bytes());
            h.update(b);
        };
        field(&KEY_VERSION.to_le_bytes());
        field(env!("SWOFTY_PIPELINE").as_bytes());
        field(self.file.path.to_string_lossy().as_bytes());
        field(&meta.len().to_le_bytes());
        field(format!("{:?}", meta.modified().ok()?).as_bytes());
        field(&self.file.erp.header);
        field(&self.file.erp.version.to_le_bytes());
        for slot in &self.slots {
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

    /// Where the archive cache in `dir` keeps this archive, under a key taken before any pending fragment
    /// is compressed.
    pub(crate) fn cache_entry(&self, dir: &Path) -> Option<ArchiveEntry> {
        Some(ArchiveEntry::at(dir, &self.key()?))
    }

    /// Writes the archive to `dst`, its pending fragments compressed.
    pub(crate) fn write_to(self, dst: &Path) -> swofty_formats::Result<()> {
        let Self { file, slots } = self;
        let entries = slots
            .into_iter()
            .map(|s| match s {
                Slot::Stock(i) => Entry::Copy(&file.erp.resources[i]),
                Slot::Owned(r) => Entry::Owned(r),
            })
            .collect();
        file.write(entries, dst)
    }

    /// The archive at `path`, every slot the game's own; `rel` names it in errors.
    pub(super) fn open(rel: &str, path: &Path) -> Result<Self> {
        let file = ErpFile::open(path).map_err(|e| fail(format!("{rel}: {e}")))?;
        let slots = (0..file.erp.resources.len()).map(Slot::Stock).collect();
        Ok(Self { file, slots })
    }

    pub(super) fn entry<'s>(&'s self, slot: &'s Slot) -> &'s Resource {
        match slot {
            Slot::Stock(i) => &self.file.erp.resources[*i],
            Slot::Owned(r) => r,
        }
    }

    pub(super) fn name<'s>(&'s self, slot: &'s Slot) -> &'s str {
        &self.entry(slot).name
    }

    pub(super) fn position(&self, name: &str) -> Result<Option<usize>> {
        let mut hits = (0..self.slots.len()).filter(|&i| self.name(&self.slots[i]) == name);
        match (hits.next(), hits.next()) {
            (Some(_), Some(_)) => Err(fail(format!("{name}: more than one resource"))),
            (hit, _) => Ok(hit),
        }
    }

    pub(super) fn find(&self, erp: &str, name: &str) -> Result<usize> {
        self.position(name)
            .map_err(|err| fail(format!("{erp}: {err}")))?
            .ok_or_else(|| Error::ResourceMissing {
                erp: erp.to_owned(),
                resource: name.to_owned(),
            })
    }

    // Read for a stock copy only: every slot still the game's, in its place.
    pub(super) fn untouched(&self) -> bool {
        self.slots.len() == self.file.erp.resources.len()
            && self
                .slots
                .iter()
                .enumerate()
                .all(|(i, s)| matches!(s, Slot::Stock(j) if *j == i))
    }

    /// The resource in slot `i`, read from the game's archive while it is the stock one.
    pub(super) fn loaded(&self, i: usize) -> Result<Cow<'_, Resource>> {
        Ok(match &self.slots[i] {
            Slot::Owned(r) => Cow::Borrowed(r),
            Slot::Stock(j) => Cow::Owned(self.file.load(&self.file.erp.resources[*j])?),
        })
    }

    /// The resources the ops wrote, in slot order.
    pub(super) fn written(&self) -> impl Iterator<Item = &Resource> {
        self.slots.iter().filter_map(|s| match s {
            Slot::Owned(r) => Some(r),
            Slot::Stock(_) => None,
        })
    }

    pub(super) fn written_mut(&mut self) -> impl Iterator<Item = &mut Resource> {
        self.slots.iter_mut().filter_map(|s| match s {
            Slot::Owned(r) => Some(r),
            Slot::Stock(_) => None,
        })
    }

    pub(super) fn owned(&mut self, i: usize) -> Result<&mut Resource> {
        if let Slot::Stock(j) = self.slots[i] {
            self.slots[i] = Slot::Owned(self.file.load(&self.file.erp.resources[j])?);
        }
        match &mut self.slots[i] {
            Slot::Owned(r) => Ok(r),
            Slot::Stock(_) => unreachable!("loaded above"),
        }
    }
}
