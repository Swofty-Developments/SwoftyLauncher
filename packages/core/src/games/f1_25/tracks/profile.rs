use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Deserialize;
use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

use crate::launcher::{ProfileTrack, ProfileTracks};
use crate::{Result, io_err};

use super::{ACTIVITIES, CONTENT_SETS, EVENTS, PlanTrack, RESOURCE_PREFIX, SEASON_SETS, TRACKS};

pub(super) struct Rows<'a> {
    by_key: HashMap<u64, &'a Record>,
}

impl<'a> Rows<'a> {
    pub(super) fn new(db: &'a Gvdb) -> Self {
        Self {
            by_key: db.records.iter().map(|r| (r.key, r)).collect(),
        }
    }

    fn text(&self, path: &str) -> Option<String> {
        self.by_key
            .get(&fnv1a64(path))
            .map(|r| r.text().trim().to_owned())
    }

    pub(super) fn table(&self, path: &str, fields: &[&str]) -> Vec<Vec<Option<String>>> {
        let (container, row) = path.rsplit_once('.').unwrap_or(("", path));
        let count = self
            .text(&format!("{container}._{row}_instancecount_"))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0);
        (0..count)
            .map(|i| {
                fields
                    .iter()
                    .map(|f| self.text(&format!("{path}_{i}.{f}")))
                    .collect()
            })
            .collect()
    }
}

pub fn list(db: &Gvdb, added: &[PlanTrack]) -> Vec<ProfileTrack> {
    let rows = Rows::new(db);
    let activities: HashMap<String, String> = rows
        .table(ACTIVITIES, &["_metadata_", "track"])
        .into_iter()
        .filter_map(|r| Some((r[0].clone()?, r[1].clone()?)))
        .collect();
    let mut events: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in rows.table(EVENTS, &["_metadata_", "activity"]) {
        let (Some(name), Some(activity)) = (r[0].clone(), r[1].clone()) else {
            continue;
        };
        let track = activities.get(&activity).cloned().unwrap_or(activity);
        events.entry(track).or_default().push(name);
    }
    let mut playable: Vec<String> = Vec::new();
    for (i, set) in rows.table(CONTENT_SETS, &["_metadata_"]).into_iter().enumerate() {
        if !set[0].as_deref().is_some_and(|l| SEASON_SETS.contains(&l)) {
            continue;
        }
        let lists = rows.table(&format!("{CONTENT_SETS}_{i}.available_tracks"), &["tracks"]);
        for list in lists.into_iter().filter_map(|r| r[0].clone()) {
            playable.extend(
                list.split(',')
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty()),
            );
        }
    }
    let mut out: Vec<ProfileTrack> = Vec::new();
    for r in rows.table(TRACKS, &["_metadata_", "statId", "track_resource"]) {
        let (Some(code), Some(stat_id)) = (r[0].clone(), r[1].as_deref().and_then(|s| s.parse::<u32>().ok()))
        else {
            continue;
        };
        if out.iter().any(|t| t.code == code) {
            continue;
        }
        let package = added.iter().find(|p| p.code == code);
        let modded = package.is_some() || r[2].as_deref() == Some(&format!("{RESOURCE_PREFIX}{code}"));
        out.push(ProfileTrack {
            events: events.remove(&code).unwrap_or_default(),
            stat_id,
            playable: playable.contains(&code),
            modded,
            name: package.and_then(|p| p.name.clone()),
            addon: package.and_then(|p| p.addon.clone()),
            code,
        });
    }
    out
}

#[derive(Deserialize)]
struct PlanListing {
    #[serde(default)]
    buildhash: Option<String>,
    #[serde(default)]
    tracks: Vec<PlanTrack>,
}

type Cached = (PathBuf, Option<SystemTime>, u64, ProfileTracks);

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

fn stamp(path: &Path) -> (Option<SystemTime>, u64) {
    std::fs::metadata(path).map_or((None, 0), |m| (m.modified().ok(), m.len()))
}

pub fn for_profile(data: &Path, game: &crate::install::Game, profile: &str) -> Result<ProfileTracks> {
    let plan = crate::cache::latest_plan(&data.join("cache/overlay").join(profile));
    let listing: Option<PlanListing> = plan.as_deref().and_then(|p| crate::read_json(p).ok());
    let built_db = plan
        .as_deref()
        .and_then(Path::parent)
        .map(|d| d.join(crate::build::GVDB))
        .filter(|p| p.is_file());
    let db_path = built_db
        .clone()
        .unwrap_or_else(|| crate::build::original(data, game, crate::build::GVDB));
    let (modified, len) = stamp(&db_path);
    let buildhash = listing.as_ref().and_then(|l| l.buildhash.clone());
    let added = listing.map(|l| l.tracks).unwrap_or_default();
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((p, m, l, tracks)) = cache.as_ref()
            && *p == db_path
            && *m == modified
            && *l == len
            && tracks.buildhash == buildhash
        {
            return Ok(tracks.clone());
        }
    }
    let bytes = std::fs::read(&db_path).map_err(io_err(&db_path))?;
    let db = Gvdb::parse(&bytes)?;
    let tracks = ProfileTracks {
        built: plan.is_some(),
        buildhash,
        tracks: list(&db, &added),
    };
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((db_path, modified, len, tracks.clone()));
    Ok(tracks)
}
