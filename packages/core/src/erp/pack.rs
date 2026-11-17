//! Every fragment the launcher changes in an archive is compressed here: the archive build's edits and the
//! map UI's surfaces. How fragments are compressed, one at a time today, changes in this file only.

use swofty_formats::erp::{Resource, STORED, ZSTD, compress_at};

use crate::{Error, Result};

/// Sets fragment `index` of `res` to `data`, compressed as the fragment was.
pub(crate) fn set_fragment(res: &mut Resource, index: usize, data: &[u8]) -> swofty_formats::Result<()> {
    res.set_fragment(index, data)
}

/// Rewrites the stale TSMD stats of `res` from its fragment 0; true when it did.
pub(crate) fn repair_stats(res: &mut Resource) -> swofty_formats::Result<bool> {
    res.repair_stats()
}

/// A resource named `name` with the layout of `template` and `data` as its fragments, compressed as the
/// template's are.
pub(crate) fn like(name: &str, template: &Resource, data: &[Vec<u8>]) -> swofty_formats::Result<Resource> {
    Resource::like(name, template, data)
}

/// Sets fragment `index` of `res` to `data` at zstd `level`, compressed when the fragment is: flagged zstd,
/// or unflagged and holding a zstd frame.
pub(crate) fn set_fragment_at(res: &mut Resource, index: usize, data: &[u8], level: i32) -> Result<()> {
    let f = res
        .fragments
        .get_mut(index)
        .ok_or_else(|| Error::Map(format!("{}: no fragment {index}", res.name)))?;
    let zstd =
        f.compression == ZSTD || (f.compression != STORED && f.raw.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]));
    f.raw = if zstd {
        compress_at(data, level)?
    } else {
        data.to_vec()
    };
    f.size = data.len() as u64;
    f.packed = f.raw.len() as u64;
    res.hash = res.content_hash()?;
    Ok(())
}
