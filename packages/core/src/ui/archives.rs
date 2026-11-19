//! Reading a resource out of a game archive and writing the archive again with patched or added resources.

use std::collections::HashMap;
use std::path::Path;

use swofty_formats::erp::{ErpFile, Packer, Resource};

use super::Patch;
use super::node::err;
use crate::erp::{Archive, Slot};
use crate::{Result, io_err};

/// How the UI build writes its archives: the packer that compresses their changed fragments and, when there
/// is one, the archive cache that keeps each archive for the next build that writes it the same.
#[derive(Clone, Copy)]
pub struct ArchiveWriter<'a> {
    packer: &'a Packer,
    cache: Option<&'a Path>,
}

impl<'a> ArchiveWriter<'a> {
    pub fn new(packer: &'a Packer) -> Self {
        Self { packer, cache: None }
    }

    /// The writer linking from the archive cache in `dir` each archive an earlier build wrote the same, and
    /// keeping there each one it writes.
    pub fn cached(self, dir: &'a Path) -> Self {
        Self {
            cache: Some(dir),
            ..self
        }
    }
}

pub(super) fn read_frag(path: &Path, name: &str, index: usize) -> Result<Vec<u8>> {
    let file = ErpFile::open(path)?;
    let res = file.load(file.erp.find(name)?)?;
    Ok(res
        .fragments
        .get(index)
        .ok_or_else(|| err(format!("{name} has no fragment {index}")))?
        .data()?)
}

pub(super) fn write_erp(
    src: &Path,
    dst: &Path,
    patches: Vec<(&str, Patch)>,
    archives: &ArchiveWriter,
) -> Result<()> {
    let patches = patches.into_iter().map(|(name, f)| (name, 0, f)).collect();
    write_archive(src, dst, patches, Vec::new(), archives)
}

pub(super) type Added = (String, &'static str, Vec<Vec<u8>>);

pub(super) fn write_archive(
    src: &Path,
    dst: &Path,
    patches: Vec<(&str, usize, Patch)>,
    added: Vec<Added>,
    archives: &ArchiveWriter,
) -> Result<()> {
    write_archive_copying(src, dst, patches, added, Vec::new(), archives)
}

/// [`write_archive`], also appending `copied`, resources of another archive kept byte for byte, under names `src`
/// doesn't have. An archive the archive cache has from an earlier build is linked instead of written again.
pub(super) fn write_archive_copying(
    src: &Path,
    dst: &Path,
    patches: Vec<(&str, usize, Patch)>,
    added: Vec<Added>,
    copied: Vec<Resource>,
    archives: &ArchiveWriter,
) -> Result<()> {
    let packer = archives.packer;
    let file = ErpFile::open(src)?;
    if let Some(r) = copied
        .iter()
        .find(|r| file.erp.resources.iter().any(|s| s.name == r.name))
    {
        return Err(err(format!("{} has {} already", src.display(), r.name)));
    }
    let mut owned: HashMap<String, Resource> = HashMap::new();
    for (name, index, f) in patches {
        let mut res = match owned.remove(name) {
            Some(r) => r,
            None => file.load(file.erp.find(name)?)?,
        };
        let frag = res
            .fragments
            .get(index)
            .ok_or_else(|| err(format!("{name} has no fragment {index}")))?;
        let data = f(&frag.data()?)?;
        res.set_fragment_with(packer, index, &data)?;
        res.repair_stats_with(packer)?;
        owned.insert(name.to_owned(), res);
    }
    let mut extra = Vec::new();
    for (name, kind, data) in added {
        let want = swofty_formats::erp::kind(kind)?;
        let like = file
            .erp
            .resources
            .iter()
            .find(|r| r.kind == want && r.fragments.len() == data.len())
            .ok_or_else(|| {
                err(format!(
                    "{}: no {kind} resource to take {name}'s layout from",
                    src.display()
                ))
            })?;
        let res = Resource::like_with(packer, &name, like, &data)?;
        match file.erp.resources.iter().any(|r| r.name == name) {
            true => {
                owned.insert(name, res);
            }
            false => extra.push(res),
        }
    }
    let slots: Vec<Slot> = file
        .erp
        .resources
        .iter()
        .enumerate()
        .map(|(i, r)| match owned.remove(&r.name) {
            Some(new) => Slot::Owned(new),
            None => Slot::Stock(i),
        })
        .chain(extra.into_iter().chain(copied).map(Slot::Owned))
        .collect();
    let archive = Archive::new(file, slots);
    let dir = dst.parent().expect("has parent");
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    let entry = archives.cache.and_then(|c| archive.cache_entry(c));
    if entry.as_ref().is_some_and(|e| e.link_to(dst)) {
        return Ok(());
    }
    #[cfg(test)]
    WRITTEN.with(|w| w.borrow_mut().push(dst.to_owned()));
    archive.write_to(dst)?;
    if let Some(e) = entry {
        e.keep(dst);
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    pub(super) static WRITTEN: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}
