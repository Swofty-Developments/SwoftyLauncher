use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde_json::Value;
use swofty_formats::gvdb::{Gvdb, Record, instance_count_path};
use swofty_formats::hash::fnv1a64;

use crate::ops::{IdFrom, stat_id_field};
use crate::stdlib::reports;
use crate::{Loaded, Result};

use super::{ALIAS_TABLES, File, REGISTRY_TABLES, path};

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

/// The addons the store in `data` keeps vehicle, engine, team, driver or head rows for: the rows a
/// save names, so a career saved with one on can't load with it off.
pub fn save_owners(data: &Path) -> BTreeSet<String> {
    let file: File = match crate::load_json(&path(data)) {
        Ok(Loaded::Found(f)) => f,
        _ => File::default(),
    };
    let held = |table: &str| {
        (table.starts_with("entrants.") && !table.starts_with("entrants.entrant_lists"))
            || table.contains("engine")
    };
    file.rows
        .keys()
        .chain(file.profiles.values().flat_map(|p| p.rows.keys()))
        .filter_map(|key| {
            let mut parts = key.splitn(3, '/');
            let (addon, table) = (parts.next()?, parts.next()?);
            held(table).then(|| addon.to_owned())
        })
        .collect()
}

/// The addons that own the team, vehicle and driver ids a save can hold ([`id_owners`]).
#[derive(Debug, Default, Clone)]
pub struct IdOwners {
    now: BTreeMap<(String, i64), BTreeSet<String>>,
    careers: BTreeMap<String, BTreeMap<(String, i64), BTreeSet<String>>>,
}

impl IdOwners {
    /// The addons that own `id` of `table` for `career`: the record the core's `ids.json` keeps for
    /// that career when it keeps one, else the addons that hold the id now, else the record another
    /// career keeps. An id an addon held before under `pin` is never its id.
    pub fn of(&self, career: &str, table: &str, id: i64) -> Option<&BTreeSet<String>> {
        let key = (table.to_owned(), id);
        self.careers
            .get(career)
            .and_then(|kept| kept.get(&key))
            .or_else(|| self.now.get(&key))
            .or_else(|| self.careers.values().find_map(|kept| kept.get(&key)))
    }
}

impl From<BTreeMap<(String, i64), BTreeSet<String>>> for IdOwners {
    fn from(now: BTreeMap<(String, i64), BTreeSet<String>>) -> Self {
        Self {
            now,
            careers: BTreeMap::new(),
        }
    }
}

/// The addons that own each team, vehicle or driver id a save can hold: the ids the store keeps for
/// their rows, the ids those rows had before, the ids the core's `ids.json` gives their team, vehicle
/// and driver records now, and the ids it keeps for each career.
pub fn id_owners(data: &Path) -> IdOwners {
    let file: File = match crate::load_json(&path(data)) {
        Ok(Loaded::Found(f)) => f,
        _ => File::default(),
    };
    let mut out = IdOwners::default();
    let mut put = |key: &str, id: i64| {
        let mut parts = key.splitn(3, '/');
        if let (Some(addon), Some(table)) = (parts.next(), parts.next())
            && ALIAS_TABLES.iter().any(|(t, _)| *t == table)
        {
            out.now
                .entry((table.to_owned(), id))
                .or_default()
                .insert(addon.to_owned());
        }
    };
    let profiles = file.profiles.values();
    for (key, entry) in file
        .rows
        .iter()
        .chain(profiles.clone().flat_map(|p| p.rows.iter()))
    {
        put(key, entry.id);
    }
    for (key, ids) in file.aliases.iter().chain(profiles.flat_map(|p| p.aliases.iter())) {
        for id in ids {
            put(key, *id);
        }
    }
    let Some(doc) = std::fs::read(reports::path(data, reports::IDS))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return out;
    };
    let held = REGISTRY_TABLES
        .iter()
        .filter(|(_, table)| ALIAS_TABLES.iter().any(|(t, _)| t == table));
    for (registry, table) in held {
        let records = |of: &Value| -> Vec<(i64, String)> {
            of[*registry]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(record, e)| {
                    let id = e.as_i64().or_else(|| e["id"].as_i64())?;
                    Some((id, record.split_once('/')?.0.to_owned()))
                })
                .collect()
        };
        for (id, addon) in records(&doc["ids"]) {
            out.now
                .entry(((*table).to_owned(), id))
                .or_default()
                .insert(addon);
        }
        for (career, kept) in doc["careers"].as_object().into_iter().flatten() {
            for (id, addon) in records(kept) {
                out.careers
                    .entry(career.clone())
                    .or_default()
                    .entry(((*table).to_owned(), id))
                    .or_default()
                    .insert(addon);
            }
        }
    }
    out
}

/// The addons the store in `data` keeps a row of `table` for, by the row's label.
pub fn label_owners(data: &Path, table: &str) -> BTreeMap<String, BTreeSet<String>> {
    let file: File = match crate::load_json(&path(data)) {
        Ok(Loaded::Found(f)) => f,
        _ => File::default(),
    };
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for key in file
        .rows
        .keys()
        .chain(file.profiles.values().flat_map(|p| p.rows.keys()))
    {
        let mut parts = key.splitn(3, '/');
        if let (Some(addon), Some(t), Some(label)) = (parts.next(), parts.next(), parts.next())
            && t == table
        {
            out.entry(label.to_owned()).or_default().insert(addon.to_owned());
        }
    }
    out
}

/// The highest `stat_id` the stock database `db` gives a team, vehicle or driver row, by table
/// (`entrants.teams.team`): an id above it in a save names a row an addon added. A table `db`
/// holds no rows of is left out.
pub fn stock_max_ids(db: &Gvdb) -> BTreeMap<String, i64> {
    let tables: Vec<(&str, &str, &str)> = ALIAS_TABLES
        .iter()
        .filter_map(|(full, _)| {
            let (table, row) = full.rsplit_once('.')?;
            Some((*full, table, row))
        })
        .collect();
    let text = |found: &HashMap<u64, &Record>, path: &str| {
        found.get(&fnv1a64(path)).map(|r| r.text().trim().to_owned())
    };
    let count_path = |table: &str, row: &str| instance_count_path(table, row);
    let counted = db.get_keys(&tables.iter().map(|(_, t, r)| fnv1a64(count_path(t, r))).collect());
    let count = |table: &str, row: &str| {
        text(&counted, &count_path(table, row))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
    };
    let id_path =
        |table: &str, row: &str, i: usize| format!("{table}.{row}_{i}.{}", stat_id_field(table, row));
    let ids = db.get_keys(
        &tables
            .iter()
            .flat_map(|&(_, t, r)| (0..count(t, r)).map(move |i| fnv1a64(id_path(t, r, i))))
            .collect(),
    );
    let mut out = BTreeMap::new();
    for (full, table, row) in tables {
        let max = (0..count(table, row))
            .filter_map(|i| text(&ids, &id_path(table, row, i))?.parse::<i64>().ok())
            .max();
        if let Some(max) = max {
            out.insert(full.to_owned(), max);
        }
    }
    out
}
