use std::collections::BTreeMap;
use std::path::PathBuf;

use super::overlay_names;
use crate::asset::Source;
use crate::content::pkg_edit::{edit_name, sources, supported};
use crate::ops::ErpOp;

pub fn reads(op: &ErpOp) -> Vec<&str> {
    match op {
        ErpOp::PatchPkg { erp, edits, .. } => std::iter::once(erp.as_str())
            .chain(sources(edits).map(|t| t.erp.as_str()))
            .collect(),
        ErpOp::CopyResource { from_erp, erp, .. } => {
            std::iter::once(erp.as_str()).chain(from_erp.as_deref()).collect()
        }
        ErpOp::PutResource { erp, .. }
        | ErpOp::ReplaceResource { erp, .. }
        | ErpOp::AddResource { erp, .. }
        | ErpOp::RemoveResource { erp, .. }
        | ErpOp::MergeSlices { erp, .. }
        | ErpOp::AppendSlices { erp, .. }
        | ErpOp::SetMaterial { erp, .. } => vec![erp.as_str()],
        ErpOp::Overlay { erps, .. } => erps.iter().map(String::as_str).collect(),
        ErpOp::AddMipmaps { .. } => Vec::new(),
    }
}

/// The archives `op` changes.
pub fn writes(op: &ErpOp) -> Vec<&str> {
    match op {
        ErpOp::PutResource { erp, .. }
        | ErpOp::ReplaceResource { erp, .. }
        | ErpOp::AddResource { erp, .. }
        | ErpOp::CopyResource { erp, .. }
        | ErpOp::RemoveResource { erp, .. }
        | ErpOp::PatchPkg { erp, .. }
        | ErpOp::MergeSlices { erp, .. }
        | ErpOp::AppendSlices { erp, .. }
        | ErpOp::SetMaterial { erp, .. } => vec![erp.as_str()],
        ErpOp::Overlay { erps, .. } => erps.iter().map(String::as_str).collect(),
        ErpOp::AddMipmaps { .. } => Vec::new(),
    }
}

pub(super) fn replaces(op: &ErpOp) -> Option<(&str, &str)> {
    match op {
        ErpOp::PutResource {
            erp,
            resource,
            rename,
            ..
        }
        | ErpOp::CopyResource {
            erp,
            resource,
            rename,
            ..
        } => Some((erp, rename.as_deref().unwrap_or(resource))),
        ErpOp::ReplaceResource { erp, resource, .. } | ErpOp::RemoveResource { erp, resource } => {
            Some((erp, resource))
        }
        _ => None,
    }
}

pub(super) fn whole_write(op: &ErpOp) -> Option<(&str, &str)> {
    match op {
        ErpOp::ReplaceResource {
            erp,
            resource,
            fragment: 0,
            ..
        } => Some((erp, resource)),
        ErpOp::PutResource { .. } | ErpOp::CopyResource { source: Some(_), .. } => replaces(op),
        _ => None,
    }
}

pub fn addon_of(origin: &str) -> &str {
    origin.split_once(" (").map_or(origin, |(id, _)| id)
}

/// The UI resources more than one addon replaces whole in `erp_ops`, by archive and name.
pub fn contested_ui(
    ops: &[crate::asset::Op],
    dir: &dyn Fn(&crate::asset::Op, &Source) -> PathBuf,
) -> std::collections::BTreeSet<(String, String)> {
    let mut by: BTreeMap<(String, String), std::collections::BTreeSet<&str>> = BTreeMap::new();
    for op in ops {
        let crate::asset::AssetOp::Erp(e) = &op.op else {
            continue;
        };
        let id = op.origin.addon.as_str();
        match e {
            ErpOp::Overlay { erps, source } => {
                let Ok(path) = source.resolve(&dir(op, source)) else {
                    continue;
                };
                for name in overlay_names(&path).unwrap_or_default() {
                    if crate::ui::merge::Kind::of(&name).is_some() {
                        for erp in erps {
                            by.entry((erp.clone(), name.clone())).or_default().insert(id);
                        }
                    }
                }
            }
            e => {
                if let Some((erp, resource)) =
                    whole_write(e).filter(|(_, r)| crate::ui::merge::Kind::of(r).is_some())
                {
                    by.entry((erp.to_owned(), resource.to_owned()))
                        .or_default()
                        .insert(id);
                }
            }
        }
    }
    by.into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(k, _)| k)
        .collect()
}

pub fn unsupported(op: &ErpOp) -> Option<&'static str> {
    match op {
        ErpOp::PatchPkg { edits, .. } => edits.iter().find(|e| !supported(e)).map(edit_name),
        _ => None,
    }
}
