use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_addon::asset::Origin;

use crate::content::track_package::{TrackPackage, Unusable};
use crate::stdlib::deps::Std;
use crate::store::Store;

/// The registry of track records (`registry/track.json`, `tracks::tracks`).
pub const TRACK_REGISTRY: &str = "track";

/// A `track` record an addon's data file adds under its own id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTrack {
    pub id: String,
    pub code: String,
    pub name: Option<String>,
    pub base: Option<String>,
    pub slot: Option<String>,
    pub files: Option<String>,
}

/// The `track` records the addon's data files (`data_files`, relative to `dir`) add, in file
/// then key order. A record without a `code` is skipped.
pub fn record_tracks(dir: &Path, addon: &str, data_files: &[String]) -> Vec<RecordTrack> {
    let mut out = Vec::new();
    for rel in data_files {
        let Some(doc) = std::fs::read_to_string(dir.join(rel))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        else {
            continue;
        };
        let mut records: Vec<(&String, &serde_json::Value)> = doc
            .get(TRACK_REGISTRY)
            .and_then(|t| t.as_object())
            .into_iter()
            .flatten()
            .collect();
        records.sort_by_key(|(k, _)| *k);
        for (key, value) in records {
            if key.contains('/') || key.starts_with('$') {
                continue;
            }
            let s = |field: &str| value.get(field).and_then(|v| v.as_str()).map(str::to_owned);
            let Some(code) = s("code") else {
                continue;
            };
            out.push(RecordTrack {
                id: format!("{addon}/{key}"),
                code,
                name: s("name"),
                base: s("base"),
                slot: s("slot"),
                files: s("files"),
            });
        }
    }
    out
}

fn stock_code(id: &str) -> Option<&str> {
    id.strip_prefix("game/").filter(|c| !c.is_empty())
}

/// The package a track record's `files` folder holds, for the build to place: its
/// `manifest.json` when it has one, else the record's code, name, base and slot with
/// `<code>.erp` and the folder's `mipmaps/`.
pub fn record_package(
    addon_dir: &Path,
    t: &RecordTrack,
) -> std::result::Result<(PathBuf, TrackPackage), Unusable> {
    let unusable = |why: String| Unusable {
        code: Some(t.code.clone()),
        name: Some(t.name.clone().unwrap_or_else(|| t.code.clone())),
        why,
        fix: None,
    };
    let Some(rel) = t.files.as_deref() else {
        return Err(unusable(format!(
            "its track record {} names no files folder",
            t.id
        )));
    };
    let Some(inside) = crate::stdlib::paths::inside(rel) else {
        return Err(unusable(format!(
            "its track record {} names {rel}, which leaves the addon's folder",
            t.id
        )));
    };
    let dir = addon_dir.join(inside.trim_end_matches('/'));
    if dir.join(crate::content::track_package::MANIFEST_FILE).is_file() {
        let pkg = crate::content::track_package::read_package(&dir)?;
        if pkg.track_code != t.code {
            return Err(unusable(format!(
                "its track record {} has code {}, and {rel}{} is for {}",
                t.id,
                t.code,
                crate::content::track_package::MANIFEST_FILE,
                pkg.track_code
            )));
        }
        return Ok((dir, pkg));
    }
    let base = t.base.as_deref().and_then(stock_code).map(str::to_owned);
    let slot = t.slot.as_deref().and_then(stock_code).map(str::to_owned);
    if base.is_none() && slot.is_none() {
        return Err(unusable(format!(
            "its track record {} names no stock base track",
            t.id
        )));
    }
    let erp = format!("{}.erp", t.code);
    if !dir.join(&erp).is_file() {
        return Err(unusable(format!("its files folder {rel} has no {erp}")));
    }
    let mips = dir.join("mipmaps").is_dir();
    let pkg = TrackPackage {
        track_code: t.code.clone(),
        track_name: t.name.clone(),
        version: None,
        author_name: None,
        erp,
        activation: crate::content::track_package::Activation {
            copy_from_track: base,
            copy_slot: slot,
            ..Default::default()
        },
        ers: Default::default(),
        copy_directories: match mips {
            true => vec![crate::content::track_package::CopyDirectory {
                source: "mipmaps/".into(),
                destination: format!("asset_groups/environment_package/tracks/{}/mipmaps/", t.code),
            }],
            false => Vec::new(),
        },
        ui: None,
        extra: Default::default(),
    };
    pkg.validate().map_err(|e| unusable(e.to_string()))?;
    Ok((dir, pkg))
}

/// The addons in `order` with a `track` record, in a data file or their lock: `track.add` only
/// places their files, and the core adds the rows.
pub fn with_records(order: &[String], std16: &BTreeMap<String, Std>) -> BTreeSet<String> {
    order
        .iter()
        .filter(|id| std16.get(*id).is_some_and(|s| s.has_records(TRACK_REGISTRY)))
        .cloned()
        .collect()
}

pub type BuildTrack = (PathBuf, TrackPackage, Origin);

/// The packages of the track records with a `files` folder that the addons in `order` add and
/// no `track.add` of theirs places, in load order, and the ones a build leaves out.
pub fn record_packages(
    store: &Store,
    order: &[String],
    std16: &BTreeMap<String, Std>,
    added: &[BuildTrack],
) -> (Vec<BuildTrack>, Vec<(Origin, Unusable)>) {
    let (mut out, mut left_out): (Vec<BuildTrack>, Vec<(Origin, Unusable)>) = (Vec::new(), Vec::new());
    for id in order {
        let Some(s) = std16.get(id).filter(|s| !s.data_files.is_empty()) else {
            continue;
        };
        let dir = store.addons_dir().join(id);
        let origin = Origin {
            addon: id.clone(),
            item: None,
        };
        for t in record_tracks(&dir, id, &s.data_files) {
            let placed = |list: &[BuildTrack]| {
                list.iter()
                    .find(|(_, p, _)| p.track_code == t.code)
                    .map(|(_, _, o)| o.addon.clone())
            };
            match placed(added).or_else(|| placed(&out)) {
                Some(by) if by == *id => continue,
                Some(by) => {
                    left_out.push((
                        origin.clone(),
                        Unusable {
                            code: Some(t.code.clone()),
                            name: Some(t.name.clone().unwrap_or_else(|| t.code.clone())),
                            why: format!("{by} adds a track with the code {} too", t.code),
                            fix: None,
                        },
                    ));
                    continue;
                }
                None => {}
            }
            if t.files.is_none() {
                continue;
            }
            match record_package(&dir, &t) {
                Ok((d, pkg)) => out.push((d, pkg, origin.clone())),
                Err(why) => left_out.push((origin.clone(), why)),
            }
        }
    }
    (out, left_out)
}

/// Leaves the folders of the build's track packages out of their addons' files for the index:
/// the build places a track's archive, mipmaps and menu art itself.
pub fn keep_track_folders(store: &Store, tracks: &[BuildTrack], std16: &mut BTreeMap<String, Std>) {
    for (dir, _, origin) in tracks {
        let Some(s) = std16.get_mut(&origin.addon) else {
            continue;
        };
        let Ok(rel) = dir.strip_prefix(store.addons_dir().join(&origin.addon)) else {
            continue;
        };
        let prefix = format!(
            "{}/",
            rel.to_string_lossy().replace('\\', "/").trim_end_matches('/')
        );
        if prefix == "/" {
            continue;
        }
        s.files.files.retain(|f| !f.starts_with(&prefix));
    }
}
