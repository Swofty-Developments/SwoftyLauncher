//! Every fragment the launcher changes in an archive is compressed here: the archive build's edits, the
//! map UI's surfaces, the placed tracks, the ERS and handling edits, the composed HUD and the addon
//! settings menus. Each takes the build's [`Packer`], which reuses the frames earlier builds compressed;
//! the archive build's defers, leaving the zstd fragments pending until the archive's write compresses
//! them together.

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
    let before = swofty_formats::erp::frame_stats();
    let started = std::time::Instant::now();
    let settled = packer.settle(resources.into_iter().flat_map(|r| r.fragments.iter_mut()))?;
    let now = swofty_formats::erp::frame_stats();
    crate::diagnostics::phases::note(format!(
        "compress {settled} fragment(s) in {} ms: {} from the frame cache ({} MiB), {} compressed ({} MiB to {} MiB, {} ms of work)",
        started.elapsed().as_millis(),
        now.hits - before.hits,
        (now.hit_bytes - before.hit_bytes) >> 20,
        now.misses - before.misses,
        (now.miss_bytes - before.miss_bytes) >> 20,
        (now.miss_packed - before.miss_packed) >> 20,
        (now.miss_nanos - before.miss_nanos) / 1_000_000,
    ));
    Ok(settled)
}

/// Marks the frames in a frame cache, so a test can tell the frames a later build takes from it.
#[cfg(test)]
pub(crate) mod frame_cache {
    use std::path::Path;

    use swofty_formats::erp::{Erp, Fragment, Resource, ZSTD, compress, kind};

    /// An empty skippable zstd frame. A decoder skips it, so a cached frame with it appended still
    /// decodes to its data and the cache serves it, and the fragment a build takes from the cache ends
    /// with it.
    const MARK: [u8; 8] = [0x50, 0x2a, 0x4d, 0x18, 0, 0, 0, 0];

    /// Appends [`MARK`] to every frame cached in `dir`, returning how many there are.
    pub(crate) fn mark(dir: &Path) -> usize {
        let mut n = 0;
        for shard in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            for frame in std::fs::read_dir(shard.path()).into_iter().flatten().flatten() {
                let mut raw = std::fs::read(frame.path()).unwrap();
                raw.extend_from_slice(&MARK);
                std::fs::write(frame.path(), raw).unwrap();
                n += 1;
            }
        }
        n
    }

    /// Whether a fragment's compressed bytes came from a cache [`mark`] marked.
    pub(crate) fn marked(raw: &[u8]) -> bool {
        raw.ends_with(&MARK)
    }

    /// How many fragments of an archive came from a cache [`mark`] marked.
    pub(crate) fn marked_in(archive: &[u8]) -> usize {
        Erp::parse(archive)
            .unwrap()
            .resources
            .iter()
            .flat_map(|r| &r.fragments)
            .filter(|f| f.raw().is_some_and(marked))
            .count()
    }

    /// A resource of `kind` with one zstd fragment holding `data`.
    pub(crate) fn zstd_resource(name: &str, of: &str, data: &[u8]) -> Resource {
        let fragment = Fragment::new(*b"temp", data.len() as u64, 0x10, ZSTD, compress(data).unwrap());
        let mut r = Resource {
            name: name.into(),
            kind: kind(of).unwrap(),
            unk: vec![0; 4],
            fragments: vec![fragment],
            hash: [0; 16],
        };
        r.hash = r.content_hash().unwrap();
        r
    }
}
