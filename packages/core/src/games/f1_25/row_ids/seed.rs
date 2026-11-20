use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use swofty_formats::gvdb::{Gvdb, instance_count_path};
use swofty_formats::hash::fnv1a64;

use crate::ops::stat_id_field;

/// What a build left of its database: its overlay's database file `rel`, when the build wrote one, and
/// its `swofty/assets.json`, whose database edits the core applies in game when the build routes the
/// addons' rows there (the overlay's file then holds the tracks' rows at most, or is not written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastBuild {
    pub db: Option<PathBuf>,
    pub edits: Option<PathBuf>,
}

/// The profile's last build in `overlays`, other than the build `current`; for a profile with none,
/// the newest of every profile's builds beside it.
pub fn last_build(overlays: &Path, current: &str, rel: &str) -> Option<LastBuild> {
    let of = |d: PathBuf| {
        let file = |p: PathBuf| {
            let at = std::fs::metadata(&p)
                .ok()
                .filter(|m| m.is_file())?
                .modified()
                .ok()?;
            Some((at, p))
        };
        let db = file(d.join(rel));
        let edits = file(d.join(crate::content::runtime_assets::ASSETS_FILE));
        let at = db.iter().chain(&edits).map(|(at, _)| *at).max()?;
        Some((
            at,
            LastBuild {
                db: db.map(|(_, p)| p),
                edits: edits.map(|(_, p)| p),
            },
        ))
    };
    let newest = |dirs: Vec<PathBuf>| {
        dirs.into_iter()
            .filter_map(of)
            .max_by_key(|(at, _)| *at)
            .map(|(_, b)| b)
    };
    let subdirs = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
            .unwrap_or_default()
    };
    let own = subdirs(overlays)
        .into_iter()
        .filter(|d| d.file_name().is_some_and(|n| n != current))
        .collect();
    newest(own).or_else(|| {
        let profiles = subdirs(overlays.parent()?);
        newest(
            profiles
                .iter()
                .filter(|p| p.as_path() != overlays)
                .flat_map(|p| subdirs(p))
                .collect(),
        )
    })
}

#[derive(Deserialize)]
struct Assets {
    #[serde(default)]
    db_edits: Vec<Edit>,
}

#[derive(Deserialize)]
struct Edit {
    hash: String,
    value: String,
}

// The last build's database as the game loads it: the overlay's file, with the build's runtime edits over it.
struct LastDb {
    db: Option<(Gvdb, HashMap<u64, usize>)>,
    edits: HashMap<u64, String>,
}

impl LastDb {
    pub(super) fn read(last: &LastBuild) -> Option<Self> {
        let db = last
            .db
            .as_ref()
            .and_then(|p| Gvdb::parse(&std::fs::read(p).ok()?).ok())
            .map(|db| {
                // Last match: the game loads a key's last record over its first (Gdb_HashMap_Insert updates in place).
                let index = db.records.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
                (db, index)
            });
        let edits: HashMap<u64, String> = last
            .edits
            .as_ref()
            .and_then(|p| serde_json::from_slice::<Assets>(&std::fs::read(p).ok()?).ok())
            .map(|a| {
                a.db_edits
                    .into_iter()
                    .filter_map(|e| Some((u64::from_str_radix(&e.hash, 16).ok()?, e.value)))
                    .collect()
            })
            .unwrap_or_default();
        (db.is_some() || !edits.is_empty()).then_some(Self { db, edits })
    }

    pub(super) fn text(&self, path: &str) -> Option<String> {
        let key = fnv1a64(path);
        if let Some(v) = self.edits.get(&key) {
            return Some(v.clone());
        }
        let (db, index) = self.db.as_ref()?;
        index.get(&key).map(|&i| db.records[i].text().into_owned())
    }
}

#[derive(Default)]
pub(super) struct Seed {
    pub(super) last: Option<LastBuild>,
    db: Option<Option<LastDb>>,
    pub(super) stock_rows: HashMap<String, usize>,
    tables: HashMap<String, HashMap<String, Vec<i64>>>,
}

impl Seed {
    // The id of the `nth` row labelled `label` that the last build added to `table`'s rows named `row`.
    pub(super) fn id(&mut self, table: &str, row: &str, label: &str, nth: usize) -> Option<i64> {
        let last = self.last.as_ref()?;
        let db = self.db.get_or_insert_with(|| LastDb::read(last)).as_ref()?;
        let name = format!("{table}.{row}");
        let skip = self.stock_rows.get(&name).copied().unwrap_or(0);
        let rows = self.tables.entry(name).or_insert_with(|| {
            let field = stat_id_field(table, row);
            let n = db
                .text(&instance_count_path(table, row))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut out: HashMap<String, Vec<i64>> = HashMap::new();
            for i in skip..n {
                let label = db.text(&format!("{table}.{row}_{i}._metadata_"));
                let id = db
                    .text(&format!("{table}.{row}_{i}.{field}"))
                    .and_then(|v| v.trim().parse().ok());
                if let (Some(label), Some(id)) = (label, id) {
                    out.entry(label).or_default().push(id);
                }
            }
            out
        });
        rows.get(label)?.get(nth).copied()
    }
}
