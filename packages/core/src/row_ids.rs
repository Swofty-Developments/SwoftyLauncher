//! The install's ids for the rows `add-row` gives an id of its own, in `$DATA/registry/row-ids.json`
//! next to the core's `ids.json` (docs/std/registry.md "Stat ids"). A key names an addon's row,
//! `<addon>/<table>.<row>/<label>`, and keeps its id at every build of every profile, whatever else
//! the build adds, since careers in the shared Steam save hold the ids.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use swofty_formats::gvdb::{Gvdb, instance_count_path};
use swofty_formats::hash::fnv1a64;

use crate::ops::{
    Claimed, DbOp, DbOps, IdFrom, RowIds, StatIdMode, add_row_name, explicit_stat_ids, hash_pool, hash_probe,
    ordinal, row_shape, stat_id_ceiling, stat_id_field, stat_id_unusable,
};
use crate::stdlib::reports;
use crate::{Loaded, Result, io_err};

pub const FILE: &str = "row-ids.json";
const LOCK: &str = "row-ids.lock";
const VERSION: u32 = 1;

/// The core's registries whose `ids.json` ids are ids of these tables' rows.
const REGISTRY_TABLES: &[(&str, &str)] = &[
    ("vehicle", "entrants.vehicles.vehicle"),
    (
        "vehicle.package",
        "entrants.vehicles.vehicle_packages.vehicle_package",
    ),
    ("content.package", "entrants.entrant_lists.entrant_list"),
    ("content.package.scenario", "game_setup.scenarios.scenario"),
    ("team.identity", "entrants.teams.team"),
    ("driver.identity", "entrants.drivers.driver"),
    ("driver.head", "entrants.animation.driver_heads.driver_head"),
];

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    rows: BTreeMap<String, Entry>,
    /// The profiles whose last build before the store seeded it.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    seeded: BTreeSet<String>,
}

// A row key the build plans an id for, its label, and which row with that label of its addon it is.
type Planned = (String, String, usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    id: i64,
    from: IdFrom,
}

/// One row the store keeps an id for: its table, label, id and how it got it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub table: String,
    pub label: String,
    pub id: i64,
    pub from: IdFrom,
}

/// The rows of `addon` the store in `data` keeps ids for, by table and label.
pub fn kept_rows(data: &Path, addon: &str) -> Result<Vec<Kept>> {
    let file: File = match crate::load_json(&path(data))? {
        Loaded::Found(f) => f,
        Loaded::Missing | Loaded::Damaged(_) => File::default(),
    };
    let prefix = format!("{addon}/");
    Ok(file
        .rows
        .into_iter()
        .filter_map(|(key, e)| {
            let (table, label) = key.strip_prefix(&prefix)?.split_once('/')?;
            Some(Kept {
                table: table.to_owned(),
                label: label.to_owned(),
                id: e.id,
                from: e.from,
            })
        })
        .collect())
}

pub fn path(data: &Path) -> PathBuf {
    reports::path(data, FILE)
}

/// Held while a build reads and writes `row-ids.json`; another build waits until it is dropped.
pub fn lock(data: &Path) -> Result<std::fs::File> {
    let dir = reports::dir(data);
    std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let path = dir.join(LOCK);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io_err(&path))?;
    file.lock().map_err(io_err(&path))?;
    Ok(file)
}

/// The database file `rel` of the profile's last build in `overlays`, other than the build `current`;
/// for a profile with none, the newest of every profile's builds beside it.
pub fn last_build(overlays: &Path, current: &str, rel: &str) -> Option<PathBuf> {
    let newest = |dirs: Vec<PathBuf>| {
        dirs.into_iter()
            .map(|d| d.join(rel))
            .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
            .max()
            .map(|(_, p)| p)
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

#[derive(Default)]
struct Seed {
    path: Option<PathBuf>,
    db: Option<Option<(Gvdb, HashMap<u64, usize>)>>,
    tables: HashMap<String, HashMap<String, Vec<i64>>>,
}

impl Seed {
    // The id of the `nth` row labelled `label` in the last build's `table` rows named `row`.
    fn id(&mut self, table: &str, row: &str, label: &str, nth: usize) -> Option<i64> {
        let path = self.path.as_ref()?;
        let (db, index) = self
            .db
            .get_or_insert_with(|| {
                let db = Gvdb::parse(&std::fs::read(path).ok()?).ok()?;
                let index = db.records.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
                Some((db, index))
            })
            .as_ref()?;
        let rows = self.tables.entry(format!("{table}.{row}")).or_insert_with(|| {
            let text = |p: &str| index.get(&fnv1a64(p)).map(|&i| db.records[i].text().into_owned());
            let field = stat_id_field(table, row);
            let n = text(&instance_count_path(table, row))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut out: HashMap<String, Vec<i64>> = HashMap::new();
            for i in 0..n {
                let label = text(&format!("{table}.{row}_{i}._metadata_"));
                let id = text(&format!("{table}.{row}_{i}.{field}")).and_then(|v| v.trim().parse().ok());
                if let (Some(label), Some(id)) = (label, id) {
                    out.entry(label).or_default().push(id);
                }
            }
            out
        });
        rows.get(label)?.get(nth).copied()
    }
}

/// The kept ids, the ids the core's registry holds, and what this build claimed and changed.
#[derive(Default)]
pub struct Store {
    rows: BTreeMap<String, i64>,
    from: HashMap<String, IdFrom>,
    planned: HashMap<String, i64>,
    adds: HashMap<String, usize>,
    seeded: BTreeSet<String>,
    by_id: HashMap<String, BTreeMap<i64, String>>,
    pinned: HashMap<String, HashSet<i64>>,
    seed: Seed,
    claimed: HashSet<String>,
    building: HashSet<String>,
    added: usize,
    moved: usize,
    reused: usize,
    changed: bool,
    read_only: bool,
}

fn addon_of(key: &str) -> &str {
    key.split('/').next().unwrap_or(key)
}

impl Store {
    /// The store in `data`, the ids the core's `ids.json` holds (in `data`, else in the game folder
    /// `game`), and, at `profile`'s first build with the store, its last build's database `last` to
    /// seed rows the store has no id for. The strings are warnings.
    pub fn load(
        data: &Path,
        game: Option<&Path>,
        profile: &str,
        last: Option<PathBuf>,
    ) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut store = Self::default();
        let file = path(data);
        match crate::load_json::<File>(&file) {
            Ok(Loaded::Found(f)) => {
                store.read_only = f.version > VERSION;
                store.seeded = f.seeded;
                for (key, e) in f.rows {
                    store.from.insert(key.clone(), e.from);
                    store.insert(key, e.id);
                }
            }
            Ok(Loaded::Missing) => {}
            Ok(Loaded::Damaged(to)) => warnings.push(format!(
                "database: {FILE} was damaged and was moved to {}; added rows take the ids of the last build",
                to.display()
            )),
            Err(e) => {
                store.read_only = true;
                warnings.push(format!(
                    "database: {FILE} can't be read ({e}); it is left as it is"
                ));
            }
        }
        if let Some(ids) = reports::found(data, game, reports::IDS)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            store.pinned = registry_ids(&ids);
        }
        if store.seeded.insert(profile.to_owned()) {
            store.seed.path = last;
            store.changed = true;
        }
        (store, warnings)
    }

    fn insert(&mut self, key: String, id: i64) {
        let table = key.split('/').nth(1).unwrap_or_default().to_owned();
        if let Some(old) = self.rows.insert(key.clone(), id)
            && let Some(ids) = self.by_id.get_mut(&table)
            && ids.get(&old) == Some(&key)
        {
            ids.remove(&old);
        }
        if let Some(prev) = self.by_id.entry(table).or_default().insert(id, key.clone())
            && prev != key
        {
            self.rows.remove(&prev);
        }
    }

    fn put(&mut self, key: &str, id: i64, from: IdFrom) {
        match self.rows.get(key) {
            Some(&kept) if kept == id => return,
            Some(_) => self.moved += 1,
            None => self.added += 1,
        }
        self.insert(key.to_owned(), id);
        self.from.insert(key.to_owned(), from);
        self.changed = true;
    }

    /// Plans the hashed ids of the rows the `add-row` ops in `files` add to `db` (rows with no
    /// kept or seeded id and a label in `set`): each row key in turn, lowest first, takes the
    /// first id from its hash that no row of `db`, explicit `stat_id`, kept id, seed or
    /// registry id holds. `stock` and `tracks` are the keys of the stock rows and the rows tracks added.
    pub fn plan<'a>(
        &mut self,
        db: &Gvdb,
        stock: &HashSet<u64>,
        tracks: &HashSet<u64>,
        files: impl IntoIterator<Item = (&'a str, &'a DbOps)>,
    ) {
        self.planned.clear();
        self.adds.clear();
        let mut keys: BTreeMap<(String, String), Vec<Planned>> = BTreeMap::new();
        let mut explicit: HashMap<String, HashSet<i64>> = HashMap::new();
        for (addon, ops) in files {
            for (table, id) in explicit_stat_ids(ops) {
                explicit.entry(table).or_default().insert(id);
            }
            let mut seen: HashMap<String, usize> = HashMap::new();
            for op in &ops.ops {
                let DbOp::AddRow {
                    table,
                    from,
                    set,
                    stat_id,
                    ..
                } = op
                else {
                    continue;
                };
                if table.contains(['[', '{']) || row_shape(table).contains("_N") {
                    continue;
                }
                let Some(row) = add_row_name(table, from) else {
                    continue;
                };
                *self.adds.entry(format!("{table}.{row}")).or_default() += 1;
                let Some(label) = set.get("_metadata_") else {
                    continue;
                };
                if set.contains_key(stat_id_field(table, row)) || *stat_id == Some(StatIdMode::Keep) {
                    continue;
                }
                let base = format!("{addon}/{table}.{row}/{label}");
                let nth = seen.entry(base.clone()).or_default();
                let key = match *nth {
                    0 => base,
                    n => format!("{base}#{}", n + 1),
                };
                keys.entry((table.clone(), row.to_owned()))
                    .or_default()
                    .push((key, label.clone(), *nth));
                *nth += 1;
            }
        }
        if keys.is_empty() {
            return;
        }
        let index: HashMap<u64, usize> = db.records.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
        let text = |key: u64| index.get(&key).map(|&i| db.records[i].text().into_owned());
        for ((table, row), mut list) in keys {
            let name = format!("{table}.{row}");
            let field = stat_id_field(&table, &row);
            let count = text(fnv1a64(instance_count_path(&table, &row)))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut fixed = explicit.remove(&name).unwrap_or_default();
            let mut top = -1;
            for i in 0..count {
                let key = fnv1a64(format!("{table}.{row}_{i}.{field}"));
                let Some(id) = text(key).and_then(|v| v.trim().parse::<i64>().ok()) else {
                    continue;
                };
                fixed.insert(id);
                if stock.contains(&key) || tracks.contains(&key) {
                    top = top.max(id);
                }
            }
            fixed.extend(self.by_id.get(&name).into_iter().flat_map(|ids| ids.keys()));
            fixed.extend(self.pinned.get(&name).into_iter().flatten());
            list.sort();
            let mut new = Vec::new();
            for (key, label, nth) in list {
                if self.rows.contains_key(&key) {
                    continue;
                }
                match self.seed.id(&table, &row, &label, nth) {
                    Some(id) => {
                        fixed.insert(id);
                    }
                    None => new.push(key),
                }
            }
            let adds = self.adds.get(&name).copied().unwrap_or(0);
            let pool = hash_pool(stat_id_ceiling(&table, &row), top, adds);
            if ordinal(pool) {
                continue;
            }
            for key in new {
                if let Some(id) = hash_probe(&key, pool)
                    .find(|id| !fixed.contains(id) && !stat_id_unusable(&table, &row, *id))
                {
                    fixed.insert(id);
                    self.planned.insert(key, id);
                }
            }
        }
    }

    /// Starts a build of `addons`; ids kept for other addons' rows are reused when a table is full.
    pub fn building(&mut self, addons: impl IntoIterator<Item = String>) {
        self.building = addons.into_iter().collect();
        self.claimed.clear();
        (self.added, self.moved, self.reused) = (0, 0, 0);
    }

    /// The id kept for `key`.
    pub fn get(&self, key: &str) -> Option<i64> {
        self.rows.get(key).copied()
    }

    /// The build line saying what the last build changed, if anything.
    pub fn summary(&self) -> Option<String> {
        if self.added + self.moved + self.reused == 0 {
            return None;
        }
        let mut parts = vec![format!("{} new", self.added)];
        if self.moved > 0 {
            parts.push(format!("{} moved", self.moved));
        }
        if self.reused > 0 {
            parts.push(format!("{} reused", self.reused));
        }
        Some(format!(
            "database: {FILE} keeps {} added-row id(s) ({})",
            self.rows.len(),
            parts.join(", ")
        ))
    }

    /// Writes the store to `data` when this build changed it.
    pub fn save(&self, data: &Path) -> Result<()> {
        if !self.changed || self.read_only {
            return Ok(());
        }
        let rows = self
            .rows
            .iter()
            .map(|(key, &id)| {
                let from = self.from.get(key).copied().unwrap_or(IdFrom::Hashed);
                (key.clone(), Entry { id, from })
            })
            .collect();
        let file = File {
            version: VERSION,
            rows,
            seeded: self.seeded.clone(),
        };
        crate::write_json(&path(data), &file)
    }
}

// The ids `ids.json` holds and its careers use, by the table whose rows they are ids of.
fn registry_ids(doc: &Value) -> HashMap<String, HashSet<i64>> {
    let mut out: HashMap<String, HashSet<i64>> = HashMap::new();
    for (registry, table) in REGISTRY_TABLES {
        let ids = out.entry((*table).to_owned()).or_default();
        if let Some(records) = doc["ids"][*registry].as_object() {
            ids.extend(
                records
                    .values()
                    .filter_map(|e| e.as_i64().or_else(|| e["id"].as_i64())),
            );
        }
        if let Some(careers) = doc["careers"].as_object() {
            for career in careers.values() {
                if let Some(records) = career[*registry].as_object() {
                    ids.extend(records.values().filter_map(Value::as_i64));
                }
            }
        }
    }
    out.retain(|_, ids| !ids.is_empty());
    out
}

/// The store as one addon's build sees it.
pub struct Scoped<'a> {
    pub addon: &'a str,
    pub store: &'a RefCell<Store>,
}

impl RowIds for Scoped<'_> {
    fn claim(&self, table: &str, row: &str, label: &str) -> Claimed {
        let mut s = self.store.borrow_mut();
        let base = format!("{}/{table}.{row}/{label}", self.addon);
        let mut nth = 0;
        let key = loop {
            let key = match nth {
                0 => base.clone(),
                n => format!("{base}#{}", n + 1),
            };
            if !s.claimed.contains(&key) {
                break key;
            }
            nth += 1;
        };
        s.claimed.insert(key.clone());
        let kept = s.get(&key);
        let seed = match kept {
            Some(_) => None,
            None => s.seed.id(table, row, label, nth),
        };
        let hashed = s.planned.get(&key).copied();
        Claimed {
            key,
            kept,
            seed,
            hashed,
        }
    }

    fn taken(&self, table: &str, row: &str, key: &str, id: i64) -> bool {
        let s = self.store.borrow();
        let table = format!("{table}.{row}");
        s.by_id
            .get(&table)
            .and_then(|ids| ids.get(&id))
            .is_some_and(|k| k != key)
            || s.pinned.get(&table).is_some_and(|ids| ids.contains(&id))
    }

    fn adds(&self, table: &str, row: &str) -> usize {
        let s = self.store.borrow();
        s.adds.get(&format!("{table}.{row}")).copied().unwrap_or(0)
    }

    fn keep(&self, _: &str, _: &str, key: &str, id: i64, from: IdFrom) {
        self.store.borrow_mut().put(key, id, from);
    }

    fn reclaim(
        &self,
        table: &str,
        row: &str,
        key: &str,
        free: &dyn Fn(i64) -> bool,
    ) -> Option<(i64, String)> {
        let table = format!("{table}.{row}");
        let spare: Vec<(i64, String)> = {
            let s = self.store.borrow();
            let pinned = s.pinned.get(&table);
            s.by_id
                .get(&table)?
                .iter()
                .filter(|(id, k)| !s.building.contains(addon_of(k)) && pinned.is_none_or(|p| !p.contains(id)))
                .map(|(id, k)| (*id, k.clone()))
                .collect()
        };
        let (id, from) = spare.into_iter().find(|(id, _)| free(*id))?;
        let mut s = self.store.borrow_mut();
        s.put(key, id, IdFrom::Reused);
        s.reused += 1;
        Some((id, from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{Features, Op, Origin};
    use crate::ops::DbReport;
    use crate::planner::Planner;
    use std::collections::BTreeSet;
    use swofty_formats::gvdb::Record;

    const VEHICLES: (&str, &str) = ("entrants.vehicles", "vehicle");
    const PACKAGES: (&str, &str) = ("entrants.vehicles.vehicle_packages", "vehicle_package");

    fn db((table, row): (&str, &str), stock: i64) -> Gvdb {
        Gvdb {
            records: vec![
                Record::new(&format!("{table}._{row}_instancecount_"), 0, "1"),
                Record::new(&format!("{table}.{row}_0._metadata_"), 4, "base"),
                Record::new(&format!("{table}.{row}_0.stat_id"), 0, stock.to_string()),
            ],
        }
    }

    struct Built {
        ids: BTreeMap<String, i64>,
        store: Store,
        lines: Vec<String>,
    }

    fn try_build(
        table: (&str, &str),
        stock: i64,
        addons: &[(&str, &[&str])],
        store: Store,
    ) -> crate::Result<Built> {
        try_build_on(db(table, stock), table, addons, store)
    }

    fn try_build_on(
        mut gvdb: Gvdb,
        (table, row): (&str, &str),
        addons: &[(&str, &[&str])],
        store: Store,
    ) -> crate::Result<Built> {
        let features = Features::default();
        let opt_in = BTreeSet::new();
        let mut p = Planner::new(&features, &opt_in);
        for (addon, labels) in addons {
            p.add_fields(
                addon,
                BTreeMap::from([(
                    format!("{table}.{row}"),
                    vec!["_metadata_".to_owned(), "stat_id".to_owned()],
                )]),
            );
            for label in *labels {
                let json = format!(
                    r#"{{"op":"db.add-row","table":"{table}","from":"{table}.{row}_0","set":{{"_metadata_":"{label}"}}}}"#
                );
                p.push(Op {
                    op: serde_json::from_str(&json).unwrap(),
                    origin: Origin {
                        addon: (*addon).to_owned(),
                        item: None,
                    },
                });
            }
        }
        p.keep_row_ids(store);
        let first = gvdb.instance_count(table, row).unwrap();
        let stock_keys = gvdb.records.iter().map(|r| r.key).collect();
        let mut report = DbReport::default();
        p.apply_db_report(&mut gvdb, &stock_keys, &HashSet::new(), &mut report)?;
        let n = gvdb.instance_count(table, row).unwrap();
        let field = stat_id_field(table, row);
        let ids = (first..n)
            .map(|i| {
                let label = gvdb.get_text(&format!("{table}.{row}_{i}._metadata_")).unwrap();
                let id = gvdb.get_text(&format!("{table}.{row}_{i}.{field}")).unwrap();
                (label, id.parse().unwrap())
            })
            .collect();
        Ok(Built {
            ids,
            store: p.take_row_ids().unwrap(),
            lines: report.lines,
        })
    }

    fn build(table: (&str, &str), stock: i64, addons: &[(&str, &[&str])], store: Store) -> Built {
        try_build(table, stock, addons, store).unwrap()
    }

    fn data_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-row-ids-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn key(addon: &str, (table, row): (&str, &str), label: &str) -> String {
        format!("{addon}/{table}.{row}/{label}")
    }

    // The ids hashing gives `rows` of `table` over stock rows up to `stock`, lowest key first, past `fixed`.
    fn hashed(
        table: (&str, &str),
        stock: i64,
        rows: &[(&str, &str)],
        fixed: &[i64],
    ) -> BTreeMap<String, i64> {
        let pool = hash_pool(stat_id_ceiling(table.0, table.1), stock, rows.len());
        let mut fixed: HashSet<i64> = fixed.iter().copied().chain([stock]).collect();
        let mut keys: Vec<(String, &str)> = rows.iter().map(|(a, l)| (key(a, table, l), *l)).collect();
        keys.sort();
        keys.into_iter()
            .map(|(k, label)| {
                let id = hash_probe(&k, pool).find(|id| !fixed.contains(id)).unwrap();
                fixed.insert(id);
                (label.to_owned(), id)
            })
            .collect()
    }

    #[test]
    fn new_ids_come_from_the_row_key_whatever_the_order() {
        let rows = [("a", "a1"), ("a", "a2"), ("b", "b1")];
        let want = hashed(VEHICLES, 10, &rows, &[]);
        let first = build(
            VEHICLES,
            10,
            &[("a", &["a1", "a2"]), ("b", &["b1"])],
            Store::default(),
        );
        assert_eq!(first.ids, want);
        assert!(
            first.lines.iter().any(|l| l.ends_with(&format!(
                ": stat_id {} (the source's is 10; from the row's key)",
                want["b1"]
            ))),
            "{:?}",
            first.lines
        );
        let other = build(
            VEHICLES,
            10,
            &[("b", &["b1"]), ("a", &["a1", "a2"])],
            Store::default(),
        );
        assert_eq!(other.ids, want);
    }

    #[test]
    fn a_small_addon_lands_in_a_window_twice_the_rows_it_adds() {
        let built = build(VEHICLES, 10, &[("a", &["a1", "a2", "a3"])], Store::default());
        assert!(
            built.ids.values().all(|id| (11..11 + 32).contains(id)),
            "{:?}",
            built.ids
        );
        assert_eq!(hash_pool(None, 10, 3), (11, 32, None));
        assert_eq!(hash_pool(None, 10, 20), (11, 64, None));
        assert_eq!(hash_pool(None, 10, 32), (11, 64, None));
        assert_eq!(hash_pool(None, 10, 33), (11, 128, None));
        let packages = stat_id_ceiling(PACKAGES.0, PACKAGES.1);
        assert_eq!(hash_pool(packages, 250, 3), (251, 4, Some(254)));
        assert_eq!(hash_pool(packages, 60, 50), (61, 128, Some(254)));
        assert_eq!(hash_pool(packages, 200, 50), (201, 54, Some(254)));
    }

    #[test]
    fn an_order_swap_keeps_each_rows_id() {
        let first = build(
            VEHICLES,
            10,
            &[("a", &["a1", "a2"]), ("b", &["b1"])],
            Store::default(),
        );
        let swapped = build(VEHICLES, 10, &[("b", &["b1"]), ("a", &["a1", "a2"])], first.store);
        assert_eq!(swapped.ids, first.ids);
        assert!(
            swapped.lines.iter().any(|l| l.ends_with(&format!(
                ": stat_id {} (the source's is 10; kept from row-ids.json)",
                first.ids["b1"]
            ))),
            "{:?}",
            swapped.lines
        );
        assert_eq!(swapped.store.summary(), None);
    }

    #[test]
    fn identical_addon_sets_built_from_different_histories_get_the_same_ids() {
        let set: &[(&str, &[&str])] = &[("a", &["a1"]), ("b", &["b1", "b2"])];
        let one = build(VEHICLES, 10, &[("a", &["a1"])], Store::default());
        let one = build(VEHICLES, 10, set, one.store);
        let two = build(VEHICLES, 10, &[("b", &["b2", "b1"])], Store::default());
        let two = build(
            VEHICLES,
            10,
            &[("x", &["x1", "x2"]), ("b", &["b1", "b2"])],
            two.store,
        );
        let two = build(VEHICLES, 10, set, two.store);
        let three = build(
            VEHICLES,
            10,
            &[("b", &["b1", "b2"]), ("a", &["a1"])],
            Store::default(),
        );
        assert_eq!(one.ids, two.ids);
        assert_eq!(one.ids, three.ids);
        assert_eq!(
            one.ids,
            hashed(VEHICLES, 10, &[("a", "a1"), ("b", "b1"), ("b", "b2")], &[])
        );
    }

    #[test]
    fn an_update_that_adds_a_row_keeps_the_old_ids_and_gives_the_new_row_a_fresh_one() {
        let first = build(
            VEHICLES,
            10,
            &[("a", &["a1", "a2"]), ("b", &["b1"])],
            Store::default(),
        );
        let updated = build(
            VEHICLES,
            10,
            &[("a", &["a0", "a1", "a2"]), ("b", &["b1"])],
            first.store,
        );
        for label in ["a1", "a2", "b1"] {
            assert_eq!(updated.ids[label], first.ids[label]);
        }
        let old: Vec<i64> = first.ids.values().copied().collect();
        assert_eq!(
            updated.ids["a0"],
            hashed(VEHICLES, 10, &[("a", "a0")], &old)["a0"]
        );
        assert_eq!(
            updated.store.summary().as_deref(),
            Some("database: row-ids.json keeps 4 added-row id(s) (1 new)")
        );
    }

    #[test]
    fn a_removed_addon_keeps_its_ids_and_gets_them_back() {
        let first = build(VEHICLES, 10, &[("a", &["a1"]), ("b", &["b1"])], Store::default());
        let without = build(VEHICLES, 10, &[("b", &["b1"]), ("c", &["c1"])], first.store);
        assert_eq!(without.ids["b1"], first.ids["b1"]);
        assert_ne!(without.ids["c1"], first.ids["a1"]);
        let back = build(
            VEHICLES,
            10,
            &[("a", &["a1"]), ("b", &["b1"]), ("c", &["c1"])],
            without.store,
        );
        assert_eq!(back.ids["a1"], first.ids["a1"]);
        assert_eq!(back.ids["b1"], first.ids["b1"]);
        assert_eq!(back.ids["c1"], without.ids["c1"]);
    }

    #[test]
    fn two_profiles_share_a_rows_id_through_the_install_store() {
        let data = data_dir("profiles");
        let (store, warnings) = Store::load(&data, None, "one", None);
        assert!(warnings.is_empty(), "{warnings:?}");
        let one = build(VEHICLES, 10, &[("a", &["a1"]), ("b", &["b1"])], store);
        one.store.save(&data).unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(path(&data)).unwrap()).unwrap();
        assert_eq!(
            saved["rows"]["b/entrants.vehicles.vehicle/b1"]["id"],
            one.ids["b1"]
        );
        assert_eq!(saved["rows"]["b/entrants.vehicles.vehicle/b1"]["from"], "hashed");
        let (store, _) = Store::load(&data, None, "two", None);
        let two = build(VEHICLES, 10, &[("b", &["b1"])], store);
        assert_eq!(two.ids["b1"], one.ids["b1"]);
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn ids_the_core_registry_holds_are_passed_over() {
        let data = data_dir("registry");
        let slot = hashed(VEHICLES, 10, &[("a", "a1")], &[])["a1"];
        put(
            &reports::path(&data, reports::IDS),
            &format!(
                r#"{{"version":1,"ids":{{"vehicle":{{"mod.x/car":{{"id":{slot},"since":"2026-10-29"}},"mod.x/old":12}}}},
                "careers":{{"dc-1":{{"vehicle":{{"mod.y/car":13}}}}}}}}"#
            ),
        );
        let (store, _) = Store::load(&data, None, "p", None);
        let first = build(VEHICLES, 10, &[("a", &["a1"])], store);
        let want = hashed(VEHICLES, 10, &[("a", "a1")], &[slot, 12, 13])["a1"];
        assert_ne!(want, slot);
        assert_eq!(first.ids["a1"], want);
        let mut kept = Store::default();
        kept.put(&key("a", VEHICLES, "a1"), 13, IdFrom::Hashed);
        kept.pinned = Store::load(&data, None, "p", None).0.pinned;
        let moved = build(VEHICLES, 10, &[("a", &["a1"])], kept);
        assert_eq!(moved.ids["a1"], want);
        assert!(
            moved.lines.iter().any(|l| l.ends_with(&format!(
                "stat_id {want} (the source's is 10; 13, kept for it, is kept for another row or held by a record of the core's registry; from the row's key)"
            ))),
            "{:?}",
            moved.lines
        );
        assert_eq!(moved.store.get(&key("a", VEHICLES, "a1")), Some(want));
        assert_eq!(moved.store.from[&key("a", VEHICLES, "a1")], IdFrom::Moved);
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn driver_ids_above_255_skip_254_255_and_low_byte_0xff() {
        const DRIVERS: (&str, &str) = ("entrants.drivers", "driver");
        let labels: Vec<String> = (0..120).map(|i| format!("d{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let built = build(DRIVERS, 254, &[("a", &refs)], Store::default());
        let ids: BTreeSet<i64> = built.ids.values().copied().collect();
        assert_eq!(ids.len(), 120);
        assert!(ids.iter().any(|id| *id > 255));
        assert!(
            ids.iter()
                .all(|id| (208..=16382).contains(id) && *id != 254 && id & 0xff != 0xff)
        );
        let again = build(DRIVERS, 254, &[("a", &refs)], built.store);
        assert_eq!(again.ids, built.ids);
    }

    #[test]
    fn a_full_table_reuses_an_id_kept_for_an_addon_the_build_leaves_out_and_logs_it() {
        let first = build(
            PACKAGES,
            250,
            &[("a", &["a1", "a2", "a3", "a4"])],
            Store::default(),
        );
        let mut ids: Vec<i64> = first.ids.values().copied().collect();
        ids.sort();
        assert_eq!(ids, [251, 252, 253, 254]);
        let e = try_build(
            PACKAGES,
            250,
            &[("a", &["a1", "a2", "a3", "a4"]), ("b", &["b1"])],
            Store::default(),
        )
        .err()
        .unwrap();
        assert!(e.to_string().contains("has no free stat_id up to 254"), "{e}");
        let holder = |id: i64| first.ids.iter().find(|(_, v)| **v == id).unwrap().0.clone();
        let b = build(PACKAGES, 250, &[("b", &["b1", "b2"])], first.store);
        assert_eq!((b.ids["b1"], b.ids["b2"]), (251, 252));
        let line = format!(
            "b: add-row entrants.vehicles.vehicle_packages.vehicle_package_1 (from entrants.vehicles.vehicle_packages.vehicle_package_0): stat_id 251 (the source's is 250; no other id up to 254 is free; reused from {}, which this build leaves out)",
            key("a", PACKAGES, &holder(251))
        );
        assert!(b.lines.contains(&line), "{:?}", b.lines);
        assert_eq!(b.store.get(&key("a", PACKAGES, &holder(251))), None);
        assert_eq!(b.store.get(&key("a", PACKAGES, &holder(253))), Some(253));
        assert_eq!(b.store.from[&key("b", PACKAGES, "b1")], IdFrom::Reused);
        assert_eq!(
            b.store.summary().as_deref(),
            Some("database: row-ids.json keeps 4 added-row id(s) (2 new, 2 reused)")
        );
    }

    #[test]
    fn rows_the_store_has_no_id_for_take_the_profiles_last_build_ids() {
        let data = data_dir("seed");
        let overlays = data.join("cache/overlay/p");
        let (table, row) = VEHICLES;
        let mut last = db(VEHICLES, 10);
        last.set(&format!("{table}._{row}_instancecount_"), "3", None)
            .unwrap();
        for (i, (label, id)) in [("a1", "40"), ("b1", "41")].iter().enumerate() {
            last.set(&format!("{table}.{row}_{}._metadata_", i + 1), *label, Some(4))
                .unwrap();
            last.set(&format!("{table}.{row}_{}.stat_id", i + 1), *id, Some(0))
                .unwrap();
        }
        let rel = "game_package/db.bin";
        put(&overlays.join("old").join(rel), "");
        std::fs::write(overlays.join("old").join(rel), last.to_bytes()).unwrap();
        assert_eq!(
            last_build(&overlays, "new", rel),
            Some(overlays.join("old").join(rel))
        );
        assert_eq!(last_build(&overlays, "old", rel), None);
        let (store, _) = Store::load(&data, None, "p", last_build(&overlays, "new", rel));
        let first = build(
            VEHICLES,
            10,
            &[("b", &["b1"]), ("a", &["a1"]), ("c", &["c1"])],
            store,
        );
        let c1 = hashed(VEHICLES, 10, &[("c", "c1")], &[40, 41])["c1"];
        assert_eq!(
            first.ids,
            BTreeMap::from([("a1".into(), 40), ("b1".into(), 41), ("c1".into(), c1)])
        );
        assert!(
            first
                .lines
                .iter()
                .any(|l| l.ends_with("stat_id 41 (the source's is 10; as the last build gave it)")),
            "{:?}",
            first.lines
        );
        assert_eq!(first.store.from[&key("a", VEHICLES, "a1")], IdFrom::Seeded);
        first.store.save(&data).unwrap();
        let seed = |profile: &str| Store::load(&data, None, profile, last_build(&overlays, "new", rel)).0;
        assert_eq!(seed("p").seed.path, None);
        assert!(seed("q").seed.path.is_some());
        let kept = kept_rows(&data, "a").unwrap();
        assert_eq!(
            kept,
            [Kept {
                table: "entrants.vehicles.vehicle".into(),
                label: "a1".into(),
                id: 40,
                from: IdFrom::Seeded
            }]
        );
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn two_rows_of_one_addon_with_one_label_keep_their_own_ids() {
        let first = build(VEHICLES, 10, &[("a", &["x", "x"])], Store::default());
        let x = first.store.get(&key("a", VEHICLES, "x")).unwrap();
        let x2 = first
            .store
            .get(&format!("{}#2", key("a", VEHICLES, "x")))
            .unwrap();
        assert_ne!(x, x2);
        let again = build(VEHICLES, 10, &[("b", &["b1"]), ("a", &["x", "x"])], first.store);
        assert_eq!(again.store.get(&key("a", VEHICLES, "x")), Some(x));
        assert_eq!(
            again.store.get(&format!("{}#2", key("a", VEHICLES, "x"))),
            Some(x2)
        );
    }

    const MODES: (&str, &str) = ("game_setup.game_setup_modes", "game_setup_mode");
    const ENTRANT_MODES: (&str, &str) = ("entrants.entrant_setup_modes", "entrant_setup_mode");

    // A table of stock rows holding the ids 0 to `top`.
    fn table_to((table, row): (&str, &str), top: i64) -> Gvdb {
        let field = stat_id_field(table, row);
        let mut records = vec![Record::new(
            &format!("{table}._{row}_instancecount_"),
            0,
            (top + 1).to_string(),
        )];
        for i in 0..=top {
            records.push(Record::new(
                &format!("{table}.{row}_{i}._metadata_"),
                4,
                format!("stock_{i}"),
            ));
            records.push(Record::new(
                &format!("{table}.{row}_{i}.{field}"),
                0,
                i.to_string(),
            ));
        }
        Gvdb { records }
    }

    #[test]
    fn mode_ids_stay_under_the_mask_and_take_the_next_free_id_in_load_order() {
        let labels: Vec<String> = (0..10).map(|i| format!("mode_{i}")).collect();
        let many: Vec<&str> = labels.iter().map(String::as_str).collect();
        let addons: &[(&str, &[&str])] = &[("bydanix", &["bdx_2012_mode"]), ("many", &many)];
        let built = try_build_on(table_to(MODES, 20), MODES, addons, Store::default()).unwrap();
        assert_eq!(built.ids["bdx_2012_mode"], 21);
        assert_eq!(built.ids["mode_9"], 31);
        assert!(built.ids.values().all(|id| *id <= 31), "{:?}", built.ids);
        assert_eq!(
            built.store.from[&key("bydanix", MODES, "bdx_2012_mode")],
            IdFrom::Next
        );
        let full: &[(&str, &[&str])] = &[
            ("bydanix", &["bdx_2012_mode"]),
            ("many", &many),
            ("more", &["one"]),
        ];
        let e = try_build_on(table_to(MODES, 20), MODES, full, Store::default())
            .err()
            .unwrap();
        assert!(e.to_string().contains("has no free stat_id up to 31"), "{e}");
        let mut kept = Store::default();
        kept.put(&key("bydanix", MODES, "bdx_2012_mode"), 44, IdFrom::Hashed);
        let moved = try_build_on(
            table_to(MODES, 20),
            MODES,
            &[("bydanix", &["bdx_2012_mode"])],
            kept,
        )
        .unwrap();
        assert_eq!(moved.ids["bdx_2012_mode"], 21);
        assert!(
            moved.lines.iter().any(|l| l.ends_with(
                "stat_id 21 (the source's is 0; 44, kept for it, is above 31, the highest the game reads)"
            )),
            "{:?}",
            moved.lines
        );
        let core_first: &[(&str, &[&str])] =
            &[("swofty.core", &["swofty_f1"]), ("bydanix", &["bdx_2012_mode"])];
        let entrants = try_build_on(
            table_to(ENTRANT_MODES, 12),
            ENTRANT_MODES,
            core_first,
            Store::default(),
        )
        .unwrap();
        assert_eq!(
            (entrants.ids["swofty_f1"], entrants.ids["bdx_2012_mode"]),
            (13, 14)
        );
        let seasons = ("game_setup.seasons", "season");
        assert_eq!(stat_id_field(seasons.0, seasons.1), "statId");
        assert_eq!(stat_id_ceiling(seasons.0, seasons.1).map(|c| c.max), Some(31));
    }

    #[test]
    fn a_new_profile_seeds_from_another_profiles_last_build() {
        let data = data_dir("other-profile");
        let overlay = data.join("cache/overlay");
        let mut old = table_to(MODES, 20);
        let (table, row) = MODES;
        old.set(&format!("{table}._{row}_instancecount_"), "22", None)
            .unwrap();
        old.set(&format!("{table}.{row}_21._metadata_"), "bdx_2012_mode", Some(4))
            .unwrap();
        old.set(&format!("{table}.{row}_21.stat_id"), "25", Some(0))
            .unwrap();
        let rel = "game_package/db.bin";
        put(&overlay.join("ig10/older").join(rel), "");
        std::fs::write(overlay.join("ig10/older").join(rel), old.to_bytes()).unwrap();
        std::fs::create_dir_all(overlay.join("fresh")).unwrap();
        let last = last_build(&overlay.join("fresh"), "new", rel);
        assert_eq!(last, Some(overlay.join("ig10/older").join(rel)));
        let (store, _) = Store::load(&data, None, "fresh", last);
        let built = try_build_on(
            table_to(MODES, 20),
            MODES,
            &[("bydanix", &["bdx_2012_mode"])],
            store,
        )
        .unwrap();
        assert_eq!(built.ids["bdx_2012_mode"], 25);
        assert!(
            built
                .lines
                .iter()
                .any(|l| l.ends_with("stat_id 25 (the source's is 0; as the last build gave it)")),
            "{:?}",
            built.lines
        );
        assert_eq!(
            built.store.from[&key("bydanix", MODES, "bdx_2012_mode")],
            IdFrom::Seeded
        );
        std::fs::remove_dir_all(&data).ok();
    }
}
