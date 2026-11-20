use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::asset::{AssetOp, FileOp, Op};
use crate::erp::Resized;

/// The warning for an addon's archive changes a build skips because the
/// archives they change aren't in the game folder.
pub(in crate::games::f1_25::build) fn missing_archives(name: &str, rels: &BTreeSet<String>) -> String {
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
pub(in crate::games::f1_25::build) fn missing_resource(
    name: &str,
    erp: &str,
    resource: &str,
    provider: &str,
) -> String {
    format!(
        "{name}: change skipped; {resource} is not in {erp}, which {provider} provides. Turn {name} off if it doesn't work, and tell its author"
    )
}

/// The refusal for two addons that add the same entry.
pub(in crate::games::f1_25::build) fn duplicate_entry(
    later: &str,
    earlier: &str,
    origin: &str,
    at: &str,
    entry: &str,
) -> String {
    format!(
        "{later} and {earlier} both add the entry {entry} ({at}); turn one of them off, or ask their authors which \
         one should own it (applying {origin})"
    )
}

/// The game-root folders addons put files in that the game never reads from
/// there: not a folder at the game root, but one under `2025_asset_groups/`.
/// Each is the addon, the folder and how many files go in it.
pub(in crate::games::f1_25::build) fn stray_asset_groups(
    game: &Path,
    files: &[Op],
) -> Vec<(String, String, usize)> {
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

pub(in crate::games::f1_25::build) fn stray_asset_group_line(name: &str, folder: &str, n: usize) -> String {
    format!(
        "{name}: {n} file(s) under {folder}/ land at the game root, where the game never reads them; the folder belongs under 2025_asset_groups/ - re-import it with that path"
    )
}

// The game loads a resource from whichever archive reads it first, so a
// resized resource that another stock archive also carries can still show
// the stock copy. Each name is matched as it sits in an archive index.
pub(in crate::games::f1_25::build) fn stock_holders(game_dir: &Path, resized: &[Resized]) -> Vec<String> {
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
        crate::erp::erp_files(&game_dir.join(&root), &root, &mut files);
        files.sort();
        let names: Vec<(&str, crate::erp::IndexName)> = uris
            .iter()
            .map(|uri| (*uri, crate::erp::IndexName::new(uri)))
            .collect();
        for rel in files {
            let Some(index) = crate::erp::index_of(&game_dir.join(&rel)) else {
                continue;
            };
            for (uri, name) in &names {
                if name.in_index(&index) && !written.contains(&(rel.to_ascii_lowercase(), *uri)) {
                    out.push(format!(
                        "erp: {uri} is resized, but {rel} also carries it and keeps the stock copy; replace it there too, or the game may show either one"
                    ));
                }
            }
        }
    }
    out
}
