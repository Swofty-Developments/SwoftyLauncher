use std::collections::BTreeSet;
use std::path::Path;

use crate::Result;
use crate::asset::{AssetOp, Op, Origin};
use crate::build::Built;
use crate::erp::{Held, SliceMerge};
use crate::planner::{Planner, Target};

pub(in crate::games::f1_25::build) fn contested_in(
    contested: &BTreeSet<(String, String)>,
    path: &Path,
    targets: &[String],
) -> Result<Option<String>> {
    if contested.is_empty() {
        return Ok(None);
    }
    let names = crate::erp::overlay_names(path)?;
    Ok(names.into_iter().find(|n| {
        targets
            .iter()
            .any(|t| contested.contains(&(t.clone(), n.clone())))
    }))
}

pub(in crate::games::f1_25::build) fn ui_merge_notes(
    built: &mut Built,
    planner: &mut Planner,
    merges: Vec<crate::erp::UiMerge>,
    name: &dyn Fn(&str) -> String,
) {
    for m in merges {
        if m.conflicts.is_empty() {
            planner.merged(&Target::Resource {
                erp: m.erp.clone(),
                uri: m.resource.clone(),
            });
        }
        let who = |ids: &[String]| ids.iter().map(|i| name(i)).collect::<Vec<_>>().join(", ");
        built.lines.push(format!(
            "ui: {}: {} merged node by node against the game's own copy: {} then {} ({} added, {} changed, {} removed by {}; {} conflict(s))",
            m.erp,
            m.resource,
            who(&m.earlier),
            name(&m.later),
            m.stats.added,
            m.stats.changed,
            m.stats.removed,
            name(&m.later),
            m.conflicts.len()
        ));
        for c in m.conflicts {
            built.warnings.push(format!(
                "ui: {} and {} both change {} in {}; {}'s version is kept there because it loads later",
                who(&c.earlier),
                name(&m.later),
                c.path,
                m.resource,
                name(&m.later)
            ));
        }
    }
}

pub(in crate::games::f1_25::build) fn slice_merges(ops: &[Op]) -> Vec<(usize, &str, &str)> {
    ops.iter()
        .enumerate()
        .filter_map(|(i, op)| match &op.op {
            AssetOp::Erp(
                crate::ops::ErpOp::MergeSlices { erp, resource, .. }
                | crate::ops::ErpOp::AppendSlices { erp, resource, .. },
            ) => Some((i, erp.as_str(), resource.as_str())),
            _ => None,
        })
        .collect()
}

// A ready overlay loads apart from the build's archives, so a later merge-slices
// into one of its resources needs it to go in resource by resource.
pub(in crate::games::f1_25::build) fn merged_later(
    slices: &[(usize, &str, &str)],
    at: usize,
    targets: &[String],
    path: &Path,
) -> Result<Option<String>> {
    let later: Vec<_> = slices
        .iter()
        .filter(|(i, erp, _)| *i > at && targets.iter().any(|t| t == erp))
        .collect();
    if later.is_empty() {
        return Ok(None);
    }
    let names = crate::erp::overlay_names(path)?;
    Ok(later
        .iter()
        .find(|(_, _, uri)| names.iter().any(|n| n == uri))
        .map(|(_, erp, uri)| format!("a later erp.merge-slices merges into {uri} in {erp}")))
}

pub(in crate::games::f1_25::build) fn kept_menus_line(name: &str) -> String {
    format!("{name} ships its own track menu pictures; the launcher's track menus are kept")
}

pub(in crate::games::f1_25::build) fn keep_menus(
    built: &mut Built,
    planner: &mut Planner,
    held: Vec<Held>,
    keeper: &dyn Fn(&Held) -> Option<Origin>,
    origin: &dyn Fn(&str) -> Option<Origin>,
    name: &dyn Fn(&str) -> String,
) {
    let mut warned = Vec::new();
    for h in held {
        let line = kept_menus_line(&name(&h.origin));
        if !warned.contains(&line) {
            built.warnings.push(line.clone());
            warned.push(line);
        }
        if let (Some(keeper), Some(lost)) = (keeper(&h), origin(&h.origin))
            && keeper.addon != lost.addon
        {
            let target = Target::Resource {
                erp: h.erp,
                uri: h.resource,
            };
            planner.kept(target, &keeper, &lost);
        }
    }
}

pub(in crate::games::f1_25::build) fn record_merge(
    built: &mut Built,
    planner: &mut Planner,
    origin: &Origin,
    m: SliceMerge,
) {
    built
        .warnings
        .extend(m.notes.iter().map(|n| format!("erp: {origin}: {n}")));
    if m.refused {
        return;
    }
    let slice = |slice: usize| Target::Slice {
        erp: m.erp.clone(),
        uri: m.resource.clone(),
        slice,
    };
    match (m.whole, m.slices) {
        (true, 0) => planner.replaced(
            Target::Resource {
                erp: m.erp.clone(),
                uri: m.resource.clone(),
            },
            origin,
        ),
        (true, n) => (0..n).for_each(|i| planner.replaced(slice(i), origin)),
        (false, n) => {
            built.lines.push(format!(
                "erp: {origin}: {} of {n} slice(s) of {} merged into {}",
                m.changed.len(),
                m.resource,
                m.erp
            ));
            m.changed.iter().for_each(|&i| planner.replaced(slice(i), origin));
        }
    }
}

/// Whether the overlay archive at `path` holds a car handling file for one of
/// a team archive in `targets`.
pub(in crate::games::f1_25::build) fn carries_handling(path: &Path, targets: &[String]) -> Result<bool> {
    if !targets.iter().any(|t| t.to_ascii_lowercase().contains("/teams/")) {
        return Ok(false);
    }
    let file = swofty_formats::erp::ErpFile::open(path)?;
    Ok(file.erp.resources.iter().any(|r| r.kind() == "VTF"))
}
