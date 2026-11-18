//! Resource overlay archives: whether a resource keeps its stock layout, and which resources share an archive.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use swofty_formats::erp::{Erp, ErpFile, Resource};

use crate::{Error, Result};

// Whether an overlay archive loads as it is over each target: no resource in it may
// change a target's stock layout. Ok with its resource count, or why not.
pub fn overlay_fits(
    path: &Path,
    targets: &[String],
    base: &dyn Fn(&str) -> PathBuf,
) -> Result<std::result::Result<usize, String>> {
    let open = |p: &Path| ErpFile::open(p).map_err(|e| Error::Other(format!("{}: {e}", p.display())));
    let overlay = open(path)?;
    for target in targets {
        let stock = open(&base(target))?;
        for res in &overlay.erp.resources {
            if let Some(why) = layout_change(res, &stock.erp) {
                return Ok(Err(format!("{} in {target}: {why}", res.name)));
            }
        }
        let find = |name: &str| -> Result<Option<Resource>> {
            for file in [&overlay, &stock] {
                if let Some(r) = file.erp.resources.iter().find(|r| r.name == name) {
                    return Ok(Some(file.load(r)?));
                }
            }
            Ok(None)
        };
        for res in &overlay.erp.resources {
            let view = match res.name.strip_suffix(".image") {
                Some(v) if v.ends_with(".texarray") => v,
                _ if res.name.ends_with(".texarray") => res.name.as_str(),
                _ => continue,
            };
            let (Some(v), Some(i)) = (find(view)?, find(&format!("{view}.image"))?) else {
                continue;
            };
            if let Some((said, has)) = crate::erp::view_mismatch(&v, &i)? {
                return Ok(Err(format!(
                    "{view} in {target}: its view says {said} slices, the image has {has}"
                )));
            }
        }
    }
    Ok(Ok(overlay.erp.resources.len()))
}

pub(super) fn same_copy(a: &Resource, b: &Resource) -> bool {
    a.kind == b.kind
        && a.unk == b.unk
        && a.fragments.len() == b.fragments.len()
        && a.fragments.iter().zip(&b.fragments).all(|(x, y)| {
            x.name == y.name
                && x.flags == y.flags
                && x.size == y.size
                && x.compression == y.compression
                && x.raw() == y.raw()
                && x.pending() == y.pending()
        })
}

pub(super) type WrittenRes<'r> = (&'r str, &'r Resource, &'r Erp, &'r str);

// The part of its origin's overlay each archive reads, by origin and archive. An
// archive joins the first part where every resource the origin wrote in it is the
// same copy the part already holds under that name, and starts a part otherwise:
// an overlay archive holds one copy of a resource for all its targets.
pub(super) fn parts_of<'r>(written: &[WrittenRes<'r>]) -> HashMap<(&'r str, &'r str), usize> {
    let mut groups: BTreeMap<(&str, &str), Vec<&Resource>> = BTreeMap::new();
    for &(erp, res, _, origin) in written {
        groups.entry((origin, erp)).or_default().push(res);
    }
    let mut held: HashMap<&str, Vec<HashMap<&str, &Resource>>> = HashMap::new();
    let mut parts = HashMap::new();
    for ((origin, erp), copies) in groups {
        let bins = held.entry(origin).or_default();
        let at = bins
            .iter()
            .position(|bin| {
                copies
                    .iter()
                    .all(|r| bin.get(r.name.as_str()).is_none_or(|held| same_copy(held, r)))
            })
            .unwrap_or(bins.len());
        if at == bins.len() {
            bins.push(HashMap::new());
        }
        for r in copies {
            bins[at].insert(r.name.as_str(), r);
        }
        parts.insert((origin, erp), at);
    }
    parts
}

pub(super) fn argument_len(erp: &Erp) -> usize {
    if erp.version >= 4 { 6 } else { 4 }
}

pub(crate) fn foreign_argument(res: &Resource, erp: &Erp) -> bool {
    res.unk.len() != argument_len(erp)
}

pub(crate) fn layout_change(res: &Resource, erp: &Erp) -> Option<String> {
    let arg = argument_len(erp);
    if res.unk.len() != arg {
        return Some(format!(
            "it has {} argument byte(s), a version {} archive uses {arg}",
            res.unk.len(),
            erp.version
        ));
    }
    let stock = erp.resources.iter().find(|s| s.name == res.name)?;
    if stock.kind != res.kind {
        return Some(format!(
            "type {} differs from the stock {}",
            res.kind(),
            stock.kind()
        ));
    }
    if stock.unk != res.unk {
        return Some("its create argument differs from the stock one".into());
    }
    if stock.fragments.len() != res.fragments.len() {
        return Some(format!(
            "it has {} fragment(s), the stock entry {}",
            res.fragments.len(),
            stock.fragments.len()
        ));
    }
    for (i, (f, s)) in res.fragments.iter().zip(&stock.fragments).enumerate() {
        if f.name != s.name || f.flags != s.flags {
            return Some(format!(
                "fragment {i} is '{}' flags {:#x}, the stock one '{}' flags {:#x}",
                f.name(),
                f.flags,
                s.name(),
                s.flags
            ));
        }
        if f.size != s.size {
            return Some(format!(
                "fragment {i} is {} bytes, the stock one {}; stock archives that bind to the replacement read it with the stock layout",
                f.size, s.size
            ));
        }
    }
    None
}
