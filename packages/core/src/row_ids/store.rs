use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde_json::Value;
use swofty_formats::gvdb::{Gvdb, instance_count_path};
use swofty_formats::hash::fnv1a64;

use crate::ops::{DbOp, DbOps, IdFrom, UNLOCK_ROW, add_row_name, row_shape, unlock_list};
use crate::stdlib::reports;
use crate::{Loaded, Result};

use super::{
    Entry, FILE, File, LastBuild, ProfileIds, Store, VERSION, career_ids, path, registry_ids, table_of,
};

impl Store {
    /// The store in `data`, the ids the core's `ids.json` holds (in `data`, else in the game folder
    /// `game`), and, at `profile`'s first build with the store, its last build's database `last` to
    /// seed rows the store has no id for. The strings are warnings.
    pub fn load(
        data: &Path,
        game: Option<&Path>,
        profile: &str,
        last: Option<LastBuild>,
    ) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut store = Self::default();
        let file = path(data);
        match crate::load_json::<File>(&file) {
            Ok(Loaded::Found(f)) => {
                store.read_only = f.version > VERSION;
                store.seeded = f.seeded;
                store.recovered = f.recovered;
                store.aliases = f.aliases.clone();
                store.base_aliases = f.aliases;
                store.existed = true;
                for (key, e) in &f.rows {
                    store
                        .base_ids
                        .entry(table_of(key).to_owned())
                        .or_default()
                        .insert(e.id, key.clone());
                }
                store.base = f.rows.clone();
                for (key, e) in f.rows {
                    store.from.insert(key.clone(), e.from);
                    store.insert(key, e.id);
                }
                let mut profiles = f.profiles;
                if let Some(mine) = profiles.remove(profile) {
                    for (key, e) in mine.rows {
                        store.from.insert(key.clone(), e.from);
                        store.local.insert(key.clone());
                        store.insert(key, e.id);
                    }
                    for (key, ids) in mine.aliases {
                        store.aliases.entry(key.clone()).or_default().extend(&ids);
                        store.local_aliases.insert(key, ids);
                    }
                }
                store.others = profiles;
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
            store.career_held = career_ids(&ids);
        }
        store.profile = profile.to_owned();
        if store.seeded.insert(profile.to_owned()) {
            store.seed.last = last;
            store.changed = true;
        }
        (store, warnings)
    }

    pub(super) fn insert(&mut self, key: String, id: i64) {
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

    pub(super) fn put(&mut self, key: &str, id: i64, from: IdFrom) {
        let table = table_of(key).to_owned();
        let kept = self.rows.get(key).copied();
        if kept == Some(id) {
            return;
        }
        let in_base = self.base.get(key).map(|e| e.id);
        let clashes = self
            .base_ids
            .get(&table)
            .and_then(|ids| ids.get(&id))
            .is_some_and(|k| k != key);
        let local = kept.is_some() || in_base.is_some() || clashes;
        match kept {
            Some(old) => {
                self.moved += 1;
                self.alias(key, old, local);
            }
            None => self.added += 1,
        }
        self.insert(key.to_owned(), id);
        self.from.insert(key.to_owned(), from);
        if in_base == Some(id) {
            self.local.remove(key);
        } else if local {
            self.local.insert(key.to_owned());
        } else {
            self.base.insert(key.to_owned(), Entry { id, from });
            self.base_ids.entry(table).or_default().insert(id, key.to_owned());
        }
        self.changed = true;
    }

    pub(super) fn base_put(&mut self, key: &str, id: i64, from: IdFrom) {
        self.base_drop(key);
        self.base.insert(key.to_owned(), Entry { id, from });
        self.base_ids
            .entry(table_of(key).to_owned())
            .or_default()
            .insert(id, key.to_owned());
    }

    pub(super) fn base_drop(&mut self, key: &str) {
        if let Some(old) = self.base.remove(key)
            && let Some(ids) = self.base_ids.get_mut(table_of(key))
            && ids.get(&old.id).is_some_and(|k| k == key)
        {
            ids.remove(&old.id);
        }
    }

    pub(super) fn alias(&mut self, key: &str, old: i64, local: bool) {
        if self.aliases.entry(key.to_owned()).or_default().insert(old) {
            self.changed = true;
        }
        let into = if local {
            &mut self.local_aliases
        } else {
            &mut self.base_aliases
        };
        into.entry(key.to_owned()).or_default().insert(old);
    }

    /// Reads, for each table the `add-row` ops in `files` add rows to, how many rows `db` holds from
    /// the stock database (`stock` holds its keys), so the seed matches only the rows the last build
    /// added. `tracks` holds the keys of the rows the tracks added.
    pub fn plan<'a>(
        &mut self,
        db: &Gvdb,
        stock: &HashSet<u64>,
        tracks: &HashSet<u64>,
        files: impl IntoIterator<Item = (&'a str, &'a DbOps)>,
    ) {
        let _ = tracks;
        self.adds_to.clear();
        let mut tables: BTreeSet<(String, String)> = BTreeSet::new();
        for (addon, ops) in files {
            for op in &ops.ops {
                let DbOp::AddRow { table, from, .. } = op else {
                    continue;
                };
                if let Some(row) = add_row_name(table, from) {
                    self.adds_to.insert((addon.to_owned(), format!("{table}.{row}")));
                }
                if let Some(list) = unlock_list(table) {
                    self.adds_to
                        .insert((addon.to_owned(), format!("{list}.{UNLOCK_ROW}")));
                }
                if table.contains(['[', '{']) || row_shape(table).contains("_N") {
                    continue;
                }
                if let Some(row) = add_row_name(table, from) {
                    tables.insert((table.clone(), row.to_owned()));
                }
            }
        }
        for (table, row) in tables {
            let count = db
                .get(&instance_count_path(&table, &row))
                .and_then(|r| r.text().trim().parse::<usize>().ok())
                .unwrap_or(0);
            let stock_rows = (0..count)
                .take_while(|i| stock.contains(&fnv1a64(format!("{table}.{row}_{i}._metadata_"))))
                .count();
            self.seed.stock_rows.insert(format!("{table}.{row}"), stock_rows);
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
    /// Whether this build gave an id, or anything else, that [`Store::save`] would write.
    pub fn unsaved(&self) -> bool {
        self.changed && !self.read_only
    }

    pub fn save(&self, data: &Path) -> Result<()> {
        if !self.unsaved() {
            return Ok(());
        }
        let mut profiles = self.others.clone();
        let rows = self
            .local
            .iter()
            .filter_map(|key| {
                let id = *self.rows.get(key)?;
                let from = self.from.get(key).copied().unwrap_or(IdFrom::Hashed);
                Some((key.clone(), Entry { id, from }))
            })
            .collect();
        let mine = ProfileIds {
            rows,
            aliases: self.local_aliases.clone(),
        };
        if !mine.is_empty() {
            profiles.insert(self.profile.clone(), mine);
        }
        let file = File {
            version: VERSION,
            rows: self.base.clone(),
            seeded: self.seeded.clone(),
            recovered: self.recovered,
            aliases: self.base_aliases.clone(),
            profiles,
        };
        crate::write_json(&path(data), &file)
    }
}
