//! Reading a resource out of a game archive and writing the archive again with patched or added resources.

use std::collections::HashMap;
use std::path::Path;

use swofty_formats::erp::{Entry, ErpFile, Packer, Resource};

use super::Patch;
use super::node::err;
use crate::{Result, io_err};

pub(super) fn read_frag(path: &Path, name: &str, index: usize) -> Result<Vec<u8>> {
    let file = ErpFile::open(path)?;
    let res = file.load(file.erp.find(name)?)?;
    Ok(res
        .fragments
        .get(index)
        .ok_or_else(|| err(format!("{name} has no fragment {index}")))?
        .data()?)
}

pub(super) fn write_erp(src: &Path, dst: &Path, patches: Vec<(&str, Patch)>, packer: &Packer) -> Result<()> {
    let patches = patches.into_iter().map(|(name, f)| (name, 0, f)).collect();
    write_archive(src, dst, patches, Vec::new(), packer)
}

pub(super) type Added = (String, &'static str, Vec<Vec<u8>>);

pub(super) fn write_archive(
    src: &Path,
    dst: &Path,
    patches: Vec<(&str, usize, Patch)>,
    added: Vec<Added>,
    packer: &Packer,
) -> Result<()> {
    write_archive_copying(src, dst, patches, added, Vec::new(), packer)
}

/// [`write_archive`], also appending `copied`, resources of another archive kept byte for byte, under names `src`
/// doesn't have.
pub(super) fn write_archive_copying(
    src: &Path,
    dst: &Path,
    patches: Vec<(&str, usize, Patch)>,
    added: Vec<Added>,
    copied: Vec<Resource>,
    packer: &Packer,
) -> Result<()> {
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
            false => extra.push(Entry::Owned(res)),
        }
    }
    let mut entries: Vec<Entry> = file
        .erp
        .resources
        .iter()
        .map(|r| match owned.remove(&r.name) {
            Some(new) => Entry::Owned(new),
            None => Entry::Copy(r),
        })
        .collect();
    entries.extend(extra);
    entries.extend(copied.into_iter().map(Entry::Owned));
    let dir = dst.parent().expect("has parent");
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    file.write(entries, dst)?;
    Ok(())
}
