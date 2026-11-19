use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_addon::asset::{AssetOp, FileKind, LangOp, OpFile, TrackOp};
use swofty_addon::manifest::AddonManifest;
use swofty_addon::ops::DbOp;

use crate::content::track_package::Unusable;
use crate::store::Store;

use super::{PlanTrack, Venues, record_tracks};

/// A data addon that is only tracks: its ops add one or more tracks and give
/// them names (`lang.set`, `lang.default`, `db.set` on a `TrackNameData` field), and it has no
/// code, screens, files, settings, launcher pages or other edits. The launcher
/// lists these on the Tracks page instead of the Addons page. An op file that
/// doesn't read makes the addon an ordinary one.
pub fn is_track_addon(dir: &Path, m: &AddonManifest) -> bool {
    let o = &m.operations;
    let other = !o.database.is_empty()
        || !o.erp.is_empty()
        || !o.ui.is_empty()
        || !o.files.is_empty()
        || !o.native.is_empty()
        || !o.screens.is_empty();
    if other || o.assets.is_empty() || m.launcher.is_some() || !m.options.is_empty() || m.saves.affects {
        return false;
    }
    let mut tracks = 0;
    for rel in &o.assets {
        let Some(doc) = std::fs::read(dir.join(rel))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
        else {
            return false;
        };
        let Ok(file) = OpFile::from_value(doc, FileKind::Assets) else {
            return false;
        };
        for op in file.ops {
            match op {
                AssetOp::Track(TrackOp::Add { .. }) => tracks += 1,
                AssetOp::Lang(LangOp::Set { .. } | LangOp::Default { .. }) | AssetOp::Warn { .. } => {}
                AssetOp::Db(DbOp::Set { field, .. }) if field.starts_with("TrackNameData") => {}
                _ => return false,
            }
        }
    }
    tracks > 0
}

// The sources of the `track.add` ops in the op files of the addon in `dir`.
fn added_sources(dir: &Path, m: &AddonManifest) -> Vec<swofty_addon::asset::Source> {
    let mut out = Vec::new();
    for rel in &m.operations.assets {
        let Some(file) = std::fs::read(dir.join(rel))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .and_then(|doc| OpFile::from_value(doc, FileKind::Assets).ok())
        else {
            continue;
        };
        for op in file.ops {
            if let AssetOp::Track(TrackOp::Add { source }) = op {
                out.push(source);
            }
        }
    }
    out
}

// The package folders an addon's `track.add` ops in its op files name.
fn added_dirs(store: &Store, id: &str, m: &AddonManifest) -> Vec<PathBuf> {
    added_sources(&store.addons_dir().join(id), m)
        .into_iter()
        .filter_map(|source| {
            source
                .resolve(&crate::build::source_dir(store, id, source.root))
                .ok()
        })
        .collect()
}

/// The `author_name` of the track packages inside the addon folder `dir` that its `track.add`
/// operations add, when they give one and the same one.
pub fn package_author(dir: &Path, m: &AddonManifest) -> Option<String> {
    let mut authors: Vec<String> = added_sources(dir, m)
        .into_iter()
        .filter(|source| source.root == swofty_addon::asset::Root::Addon)
        .filter_map(|source| source.resolve(dir).ok())
        .map(|d| {
            std::fs::read(d.join(crate::content::track_package::MANIFEST_FILE))
                .ok()
                .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
                .and_then(|doc| Some(doc.get("author_name")?.as_str()?.trim().to_owned()))
                .filter(|a| !a.is_empty())
        })
        .collect::<Option<Vec<String>>>()?;
    authors.dedup();
    match authors.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// The tracks an addon's `track.add` operations add, as the core lists them
/// (`<addon>/<code>`), with the venue each one's name matches.
pub fn older_ids(store: &Store, id: &str, m: &AddonManifest) -> Vec<(String, Option<String>)> {
    added_dirs(store, id, m)
        .iter()
        .filter_map(|d| crate::content::track_package::read_package(d).ok())
        .map(|pkg| {
            let venue = pkg
                .track_name
                .as_deref()
                .and_then(|n| Venues::shipped().by_name(n));
            (format!("{id}/{}", pkg.track_code), venue)
        })
        .collect()
}

/// The tracks installed addons add with `track.add`, named from their
/// packages, by addon id; the first addon to add a track keeps it. Ops or
/// packages that don't read are skipped.
pub fn installed(store: &Store) -> Vec<PlanTrack> {
    let mut out: Vec<PlanTrack> = Vec::new();
    for (id, m) in store.addons().unwrap_or_default() {
        for dir in added_dirs(store, &id, &m) {
            let Ok(pkg) = crate::content::track_package::read_package(&dir) else {
                continue;
            };
            if out.iter().any(|t| t.code == pkg.track_code) {
                continue;
            }
            out.push(PlanTrack {
                name: pkg.track_name.filter(|n| !n.trim().is_empty()),
                addon: Some(id.clone()),
                code: pkg.track_code,
            });
        }
    }
    out
}

/// The track packages an addon's op files add that builds leave out.
pub fn unusable(store: &Store, id: &str, m: &AddonManifest) -> Vec<Unusable> {
    added_dirs(store, id, m)
        .iter()
        .filter_map(|d| crate::content::track_package::read_package(d).err())
        .collect()
}

/// How many tracks the addons of `order` add with `track.add`, to tell whether a build can fill the lists.
pub fn added_count(store: &Store, order: &[String], addons: &BTreeMap<String, AddonManifest>) -> usize {
    order
        .iter()
        .filter_map(|id| addons.get(id).map(|m| added_dirs(store, id, m).len()))
        .sum()
}

pub fn left_out_line(addon: &str, id: &str, why: &Unusable) -> String {
    match addon == id {
        true => format!("{id}: {}", why.text()),
        false => format!("{addon} ({id}): {}", why.text()),
    }
}

/// A [`left_out_line`] for each track package `m`'s op files add that builds leave out.
pub fn left_out_lines(store: &Store, m: &AddonManifest) -> Vec<String> {
    unusable(store, &m.id, m)
        .iter()
        .map(|u| left_out_line(&m.name, &m.id, u))
        .collect()
}

/// The installed addons, by track code, whose `track.add` or `track` record adds each of
/// `codes`, also where a build would leave the track out.
pub(crate) fn adders(store: &Store, codes: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (id, m) in store.addons().unwrap_or_default() {
        for (code, _) in added_codes(store, &id, &m) {
            if codes.contains(&code) {
                out.entry(code).or_insert_with(|| id.clone());
            }
        }
    }
    out
}

/// The track codes addon `id`'s `track.add` ops and `track` records add, each with whether its package reads.
pub(super) fn added_codes(store: &Store, id: &str, m: &AddonManifest) -> Vec<(String, bool)> {
    let dir = store.addons_dir().join(id);
    let added = added_dirs(store, id, m)
        .iter()
        .filter_map(|d| match crate::content::track_package::read_package(d) {
            Ok(pkg) => Some((pkg.track_code, true)),
            Err(why) => why.code.map(|c| (c, false)),
        })
        .collect::<Vec<_>>();
    let recorded = record_tracks(&dir, id, &crate::stdlib::data::files(&dir));
    added
        .into_iter()
        .chain(recorded.into_iter().map(|t| (t.code, true)))
        .collect()
}
