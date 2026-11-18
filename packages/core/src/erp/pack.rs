//! Every fragment the launcher changes in an archive is compressed here: the archive build's edits, the
//! map UI's surfaces, the placed tracks and the ERS and handling edits. The archive build and the map UI
//! take the build's [`Packer`]; a deferring one leaves the zstd fragments pending, and the archive's write
//! compresses them together.

use swofty_formats::erp::{Packer, Resource};

use crate::{Error, Result};

/// Sets fragment `index` of `res` to `data`, compressed as the fragment was.
pub(crate) fn set_fragment(
    packer: &Packer,
    res: &mut Resource,
    index: usize,
    data: &[u8],
) -> swofty_formats::Result<()> {
    res.set_fragment_with(packer, index, data)
}

/// Rewrites the stale TSMD stats of `res` from its fragment 0; true when it did.
pub(crate) fn repair_stats(packer: &Packer, res: &mut Resource) -> swofty_formats::Result<bool> {
    res.repair_stats_with(packer)
}

/// A resource named `name` with the layout of `template` and `data` as its fragments, compressed as the
/// template's are.
pub(crate) fn like(
    packer: &Packer,
    name: &str,
    template: &Resource,
    data: &[Vec<u8>],
) -> swofty_formats::Result<Resource> {
    Resource::like_with(packer, name, template, data)
}

/// Sets fragment `index` of `res` to `data` at zstd `level`, compressed when the fragment is: flagged zstd,
/// or unflagged and holding a zstd frame.
pub(crate) fn set_fragment_at(
    packer: &Packer,
    res: &mut Resource,
    index: usize,
    data: &[u8],
    level: i32,
) -> Result<()> {
    let f = res
        .fragments
        .get_mut(index)
        .ok_or_else(|| Error::Map(format!("{}: no fragment {index}", res.name)))?;
    let zstd = f.is_zstd();
    packer.put(f, data, zstd, level)?;
    res.hash = res.content_hash()?;
    Ok(())
}

/// Compresses every fragment of `resources` still pending; how many it compressed.
pub(crate) fn settle<'r>(
    packer: &Packer,
    resources: impl IntoIterator<Item = &'r mut Resource>,
) -> Result<usize> {
    Ok(packer.settle(resources.into_iter().flat_map(|r| r.fragments.iter_mut()))?)
}
