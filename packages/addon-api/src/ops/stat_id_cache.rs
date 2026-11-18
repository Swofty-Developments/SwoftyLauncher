use std::collections::HashSet;

use swofty_formats::hash::fnv1a64;

use crate::ops::field_names::FieldNames;
use crate::ops::game_db::Db;
use crate::ops::stat_ids::{stat_id_field, stat_id_unusable};

// The stat_ids of a table's rows, read once and kept up to date by add-row until any other write.
pub(crate) struct StatIdCache {
    pub(crate) table: String,
    pub(crate) row: String,
    pub(crate) generation: u64,
    pub(crate) ids: Vec<Option<i64>>,
    pub(crate) used: HashSet<i64>,
    pub(crate) max: Option<i64>,
    // The highest id of a stock or track row.
    pub(crate) stock: i64,
    // The ids of the rows this apply didn't add, as a count and a set, to tell whether they are all distinct.
    pub(crate) kept: usize,
    pub(crate) kept_distinct: HashSet<i64>,
    // A start and the lowest id from it that no row holds and the game can read.
    pub(crate) lowest_free: Option<(i64, i64)>,
}

impl StatIdCache {
    pub(crate) fn read(db: &Db, names: &dyn FieldNames, table: &str, row: &str, rows: usize) -> Self {
        let field = stat_id_field(table, row);
        let mut ids = Self {
            table: table.to_owned(),
            row: row.to_owned(),
            generation: db.generation,
            ids: Vec::with_capacity(rows),
            used: HashSet::with_capacity(rows),
            max: None,
            stock: -1,
            kept: 0,
            kept_distinct: HashSet::new(),
            lowest_free: None,
        };
        for i in 0..rows {
            let name = format!("{table}.{row}_{i}");
            let key = fnv1a64(format!("{name}.{field}"));
            let id = db.text(key).and_then(|v| v.trim().parse::<i64>().ok());
            ids.push(names, key, id, db.added_rows.contains(&name));
        }
        ids
    }

    pub(crate) fn push(&mut self, names: &dyn FieldNames, key: u64, id: Option<i64>, added: bool) {
        self.ids.push(id);
        let Some(id) = id else {
            return;
        };
        if names.stock(key) || names.track(key) {
            self.stock = self.stock.max(id);
        }
        if !added {
            self.kept += 1;
            self.kept_distinct.insert(id);
        }
        self.used.insert(id);
        self.max = Some(self.max.map_or(id, |m| m.max(id)));
        if let Some((_, low)) = &mut self.lowest_free
            && *low == id
        {
            while self.used.contains(low) || stat_id_unusable(&self.table, &self.row, *low) {
                *low += 1;
            }
        }
    }

    // Whether the rows this apply didn't add have ids, all different.
    pub(crate) fn indexed(&self) -> bool {
        self.kept > 0 && self.kept_distinct.len() == self.kept
    }

    pub(crate) fn lowest_free(&mut self, from: i64) -> i64 {
        if let Some((start, low)) = self.lowest_free
            && start == from
        {
            return low;
        }
        let mut low = from;
        while self.used.contains(&low) || stat_id_unusable(&self.table, &self.row, low) {
            low += 1;
        }
        self.lowest_free = Some((from, low));
        low
    }
}
