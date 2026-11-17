use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::Built;
use super::source_dir;
use crate::asset::{AssetOp, FileOp, Op, Origin};
use crate::erpbuild::{Held, Resized, SliceMerge, writes};
use crate::planner::{Planner, Target};
use crate::store::Store;
use crate::{Error, Result};

pub(super) fn contested_in(
    contested: &BTreeSet<(String, String)>,
    path: &Path,
    targets: &[String],
) -> Result<Option<String>> {
    if contested.is_empty() {
        return Ok(None);
    }
    let names = crate::erpbuild::overlay_names(path)?;
    Ok(names.into_iter().find(|n| {
        targets
            .iter()
            .any(|t| contested.contains(&(t.clone(), n.clone())))
    }))
}

pub(super) fn ui_merge_notes(
    built: &mut Built,
    planner: &mut Planner,
    merges: Vec<crate::erpbuild::UiMerge>,
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

pub(super) fn slice_merges(ops: &[Op]) -> Vec<(usize, &str, &str)> {
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
pub(super) fn merged_later(
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
    let names = crate::erpbuild::overlay_names(path)?;
    Ok(later
        .iter()
        .find(|(_, _, uri)| names.iter().any(|n| n == uri))
        .map(|(_, erp, uri)| format!("a later erp.merge-slices merges into {uri} in {erp}")))
}

pub(super) fn kept_menus_line(name: &str) -> String {
    format!("{name} ships its own track menu pictures; the launcher's track menus are kept")
}

pub(super) fn keep_menus(
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

pub(super) fn record_merge(built: &mut Built, planner: &mut Planner, origin: &Origin, m: SliceMerge) {
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
pub(super) fn carries_handling(path: &Path, targets: &[String]) -> Result<bool> {
    if !targets.iter().any(|t| t.to_ascii_lowercase().contains("/teams/")) {
        return Ok(false);
    }
    let file = swofty_formats::erp::ErpFile::open(path)?;
    Ok(file.erp.resources.iter().any(|r| r.kind() == "VTF"))
}

pub(super) fn stage_erps(
    built: &mut Built,
    staged: &BTreeMap<String, PathBuf>,
    rels: Vec<String>,
) -> Result<()> {
    for rel in rels {
        let src = built.overlay.join(&rel);
        if built.plan.data.insert(rel.clone(), src).is_some() && !staged.contains_key(&rel) {
            return Err(Error::Other(format!("{rel} is replaced twice")));
        }
    }
    Ok(())
}

/// The warning for an addon's archive changes a build skips because the
/// archives they change aren't in the game folder.
pub(super) fn missing_archives(name: &str, rels: &BTreeSet<String>) -> String {
    let first = rels.first().map_or("", String::as_str);
    let what = match rels.len() {
        1 => format!("{first} is not a file in F1 25"),
        n => format!("{n} of the files it changes are not in F1 25, such as {first}"),
    };
    format!(
        "{name}: archive changes skipped; {what}. Turn {name} off if it doesn't work, and tell its author"
    )
}

/// The warning for an addon's change a build skips because the archive
/// another addon provides has no such resource.
pub(super) fn missing_resource(name: &str, erp: &str, resource: &str, provider: &str) -> String {
    format!(
        "{name}: change skipped; {resource} is not in {erp}, which {provider} provides. Turn {name} off if it doesn't work, and tell its author"
    )
}

/// The game-root folders addons put files in that the game never reads from
/// there: not a folder at the game root, but one under `2025_asset_groups/`.
/// Each is the addon, the folder and how many files go in it.
pub(super) fn stray_asset_groups(game: &Path, files: &[Op]) -> Vec<(String, String, usize)> {
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for op in files {
        let AssetOp::File(FileOp::Put { target, .. }) = &op.op else {
            continue;
        };
        let Some((top, _)) = target.split_once('/') else {
            continue;
        };
        if top.is_empty()
            || top.eq_ignore_ascii_case("2025_asset_groups")
            || game.join(top).is_dir()
            || !game.join("2025_asset_groups").join(top).is_dir()
        {
            continue;
        }
        *counts
            .entry((op.origin.addon.clone(), top.to_owned()))
            .or_default() += 1;
    }
    counts.into_iter().map(|((a, f), n)| (a, f, n)).collect()
}

pub(super) fn stray_asset_group_line(name: &str, folder: &str, n: usize) -> String {
    format!(
        "{name}: {n} file(s) under {folder}/ land at the game root, where the game never reads them; the folder belongs under 2025_asset_groups/ - re-import it with that path"
    )
}

/// An archive a `file.put` replaces whole while ERP ops change it too: its last
/// put in load order.
pub(super) struct ArchivePut {
    rank: usize,
    pub origin: Origin,
    pub path: PathBuf,
}

pub(super) fn archive_puts(
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

pub(super) fn holds_texture_arrays(path: &Path) -> bool {
    swofty_formats::erp::ErpFile::open(path).is_ok_and(|f| {
        f.erp
            .resources
            .iter()
            .any(|r| r.name.ends_with(".texarray") || r.name.ends_with(".texarray.image"))
    })
}

/// Leaves out of `op` the archives a put replaces later in load order; false
/// when none is left.
pub(super) fn after_puts(op: &mut Op, puts: &BTreeMap<String, ArchivePut>, rank: usize) -> bool {
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

// The game loads a resource from whichever archive reads it first, so a
// resized resource that another stock archive also carries can still show
// the stock copy. Each name is matched as it sits in an archive index.
pub(super) fn stock_holders(game_dir: &Path, resized: &[Resized]) -> Vec<String> {
    let mut by_root: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut written = std::collections::BTreeSet::new();
    for r in resized {
        let root: Vec<&str> = r.erp.splitn(3, '/').take(2).collect();
        for (uri, _) in &r.resources {
            written.insert((r.erp.to_ascii_lowercase(), uri.as_str()));
            by_root.entry(root.join("/")).or_default().push(uri);
        }
    }
    let mut out = Vec::new();
    for (root, mut uris) in by_root {
        uris.sort_unstable();
        uris.dedup();
        let mut files = Vec::new();
        crate::erpbuild::erp_files(&game_dir.join(&root), &root, &mut files);
        files.sort();
        for rel in files {
            let Some(index) = crate::erpbuild::index_of(&game_dir.join(&rel)) else {
                continue;
            };
            for uri in &uris {
                if crate::erpbuild::index_holds(&index, uri)
                    && !written.contains(&(rel.to_ascii_lowercase(), *uri))
                {
                    out.push(format!(
                        "erp: {uri} is resized, but {rel} also carries it and keeps the stock copy; replace it there too, or the game may show either one"
                    ));
                }
            }
        }
    }
    out
}
