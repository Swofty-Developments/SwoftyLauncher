use std::collections::{HashMap, HashSet};

use swofty_formats::gvdb::{Gvdb, Record, instance_count_path};
use swofty_formats::hash::fnv1a64;

use crate::ops::apply::Cx;
use crate::ops::field_names::FieldNames;
use crate::ops::resolve::selectors::Selectors;
use crate::ops::stat_id_cache::StatIdCache;
use crate::ops::stat_ids::stat_id_field;
use crate::{Error, Result};

pub(crate) struct Db<'a> {
    pub(crate) gvdb: &'a mut Gvdb,
    pub(crate) index: HashMap<u64, usize>,
    pub(crate) removed: Vec<usize>,
    pub(crate) added_rows: HashSet<String>,
    pub(crate) moved: HashMap<(String, String), String>,
    pub(crate) ties: std::cell::RefCell<Vec<String>>,
    // Bumped by every write, so a cached read of the records knows when it is stale.
    pub(crate) generation: u64,
    pub(crate) stat_ids: std::cell::RefCell<HashMap<(String, String), StatIdCache>>,
    pub(crate) selectors: std::cell::RefCell<Selectors>,
    /// Answers each selector by reading every row, as the kept values must.
    #[cfg(test)]
    pub(crate) scan_selectors: bool,
}

#[cfg(test)]
thread_local! {
    /// Whether the databases this test thread opens answer selectors by reading every row.
    pub(crate) static SCAN_SELECTORS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl<'a> Db<'a> {
    pub(crate) fn new(gvdb: &'a mut Gvdb) -> Self {
        let mut index = HashMap::with_capacity(gvdb.records.len());
        for (i, r) in gvdb.records.iter().enumerate() {
            index.entry(r.key).or_insert(i);
        }
        Self {
            gvdb,
            index,
            removed: Vec::new(),
            added_rows: HashSet::new(),
            moved: HashMap::new(),
            ties: std::cell::RefCell::new(Vec::new()),
            generation: 0,
            stat_ids: std::cell::RefCell::new(HashMap::new()),
            selectors: std::cell::RefCell::new(Selectors::default()),
            #[cfg(test)]
            scan_selectors: SCAN_SELECTORS.with(std::cell::Cell::get),
        }
    }

    pub(crate) fn stat_ids(
        &self,
        names: &dyn FieldNames,
        table: &str,
        row: &str,
        rows: usize,
    ) -> std::cell::RefMut<'_, StatIdCache> {
        std::cell::RefMut::map(self.stat_ids.borrow_mut(), |all| {
            let ids = all
                .entry((table.to_owned(), row.to_owned()))
                .or_insert_with(|| StatIdCache::read(self, names, table, row, rows));
            if ids.generation != self.generation || ids.ids.len() != rows {
                *ids = StatIdCache::read(self, names, table, row, rows);
            }
            ids
        })
    }

    // Adds the row `name`, the table's `rows`th, to its cached ids, when nothing but that row's writes
    // came since generation `before`.
    pub(crate) fn row_added(
        &self,
        names: &dyn FieldNames,
        table: &str,
        row: &str,
        rows: usize,
        name: &str,
        before: u64,
    ) {
        let mut all = self.stat_ids.borrow_mut();
        let Some(ids) = all.get_mut(&(table.to_owned(), row.to_owned())) else {
            return;
        };
        if ids.generation != before || ids.ids.len() != rows {
            return;
        }
        let key = fnv1a64(format!("{name}.{}", stat_id_field(table, row)));
        let id = self.text(key).and_then(|v| v.trim().parse::<i64>().ok());
        ids.push(names, key, id, true);
        ids.generation = self.generation;
    }

    pub(crate) fn drain_ties(&self, cx: &mut Cx) -> Result<()> {
        for tie in self.ties.take() {
            if cx.strict {
                return Err(Error::Resolve(format!("{tie} (strict)")));
            }
            if !cx.report.warnings.contains(&tie) {
                cx.report.warnings.push(tie);
            }
        }
        Ok(())
    }

    pub(crate) fn remove(&mut self, key: u64) -> bool {
        self.generation += 1;
        self.selectors.get_mut().forget(key);
        match self.index.remove(&key) {
            Some(i) => {
                self.removed.push(i);
                true
            }
            None => false,
        }
    }

    pub(crate) fn finish(self) {
        if self.removed.is_empty() {
            return;
        }
        let removed: HashSet<usize> = self.removed.into_iter().collect();
        let mut i = 0;
        self.gvdb.records.retain(|_| {
            i += 1;
            !removed.contains(&(i - 1))
        });
    }

    pub(crate) fn get(&self, key: u64) -> Option<&Record> {
        self.index.get(&key).map(|&i| &self.gvdb.records[i])
    }

    pub(crate) fn get_mut(&mut self, key: u64) -> Option<&mut Record> {
        self.generation += 1;
        self.selectors.get_mut().forget(key);
        self.index.get(&key).map(|&i| &mut self.gvdb.records[i])
    }

    pub(crate) fn text(&self, key: u64) -> Option<String> {
        self.get(key).map(|r| r.text().into_owned())
    }

    pub(crate) fn push(&mut self, record: Record) {
        self.generation += 1;
        self.selectors.get_mut().forget(record.key);
        self.index.insert(record.key, self.gvdb.records.len());
        self.gvdb.records.push(record);
    }

    pub(crate) fn instance_count(&self, table: &str, row: &str) -> Option<usize> {
        self.text(fnv1a64(instance_count_path(table, row)))?
            .trim()
            .parse()
            .ok()
    }
}
