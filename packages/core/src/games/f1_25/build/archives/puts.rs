use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::Result;
use crate::asset::{AssetOp, FileOp, Op, Origin};
use crate::build::source_dir;
use crate::erp::writes;
use crate::store::Store;

/// An archive a `file.put` replaces whole while ERP ops change it too: its last
/// put in load order.
pub(in crate::games::f1_25::build) struct ArchivePut {
    rank: usize,
    pub origin: Origin,
    pub path: PathBuf,
}

pub(in crate::games::f1_25::build) fn archive_puts(
    store: &Store,
    files: &[Op],
    erps: &[Op],
    rank: &dyn Fn(&Origin) -> usize,
) -> Result<BTreeMap<String, ArchivePut>> {
    let changed: BTreeSet<&str> = erps
        .iter()
        .flat_map(|op| match &op.op {
            AssetOp::Erp(e) => writes(e),
            _ => Vec::new(),
        })
        .collect();
    let mut last = BTreeMap::new();
    for op in files {
        let AssetOp::File(FileOp::Put { source, target }) = &op.op else {
            continue;
        };
        let erp = target.to_ascii_lowercase().ends_with(".erp");
        if erp || changed.contains(target.as_str()) {
            last.insert(target.clone(), (op, source));
        }
    }
    let mut puts = BTreeMap::new();
    for (target, (op, source)) in last {
        let path = source.resolve(&source_dir(store, &op.origin.addon, source.root))?;
        if changed.contains(target.as_str()) || holds_texture_arrays(&path) {
            puts.insert(
                target,
                ArchivePut {
                    rank: rank(&op.origin),
                    origin: op.origin.clone(),
                    path,
                },
            );
        }
    }
    Ok(puts)
}

pub(in crate::games::f1_25::build) fn holds_texture_arrays(path: &Path) -> bool {
    swofty_formats::erp::ErpFile::open(path).is_ok_and(|f| {
        f.erp
            .resources
            .iter()
            .any(|r| r.name.ends_with(".texarray") || r.name.ends_with(".texarray.image"))
    })
}

/// Leaves out of `op` the archives a put replaces later in load order; false
/// when none is left.
pub(in crate::games::f1_25::build) fn after_puts(
    op: &mut Op,
    puts: &BTreeMap<String, ArchivePut>,
    rank: usize,
) -> bool {
    let replaced = |erp: &str| puts.get(erp).is_some_and(|p| p.rank > rank);
    match &mut op.op {
        AssetOp::Erp(crate::ops::ErpOp::Overlay { erps, .. }) => {
            erps.retain(|e| !replaced(e));
            !erps.is_empty()
        }
        AssetOp::Erp(e) => !writes(e).into_iter().any(replaced),
        _ => true,
    }
}
