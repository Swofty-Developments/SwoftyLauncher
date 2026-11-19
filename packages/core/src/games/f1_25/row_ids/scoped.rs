use std::cell::RefCell;

use crate::ops::{Claimed, IdFrom, RowIds};

use super::{REGISTRY_TABLES, Store, UNHELD_REGISTRIES, addon_of, pool};

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
        Claimed { key, kept, seed }
    }

    fn taken(&self, table: &str, row: &str, key: &str, id: i64) -> bool {
        let s = self.store.borrow();
        let table = format!("{table}.{row}");
        s.by_id
            .get(&table)
            .and_then(|ids| ids.get(&id))
            .is_some_and(|k| k != key)
            || s.pinned
                .get(pool(&table))
                .and_then(|ids| ids.get(&id))
                .is_some_and(|record| {
                    s.pin_holds(record)
                        && !(s.spare_pin(&table, id, record)
                            && s.by_id
                                .get(&table)
                                .and_then(|ids| ids.get(&id))
                                .is_some_and(|k| k == key))
                })
    }

    fn keep(&self, _: &str, _: &str, key: &str, id: i64, from: IdFrom) {
        self.store.borrow_mut().put(key, id, from);
    }

    fn registry_record(&self, table: &str, row: &str, label: Option<&str>, id: i64) -> Option<String> {
        let s = self.store.borrow();
        let table = format!("{table}.{row}");
        let unheld = REGISTRY_TABLES
            .iter()
            .any(|(registry, t)| *t == table && UNHELD_REGISTRIES.contains(registry));
        let record = s.pinned.get(&table).filter(|_| !unheld)?.get(&id)?;
        let own = addon_of(record) == self.addon || record.rsplit('/').next() == label;
        (s.pin_holds(record) && !own).then(|| record.clone())
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
            let pinned = s.pinned.get(pool(&table));
            let mut spare: Vec<(i64, String)> = s
                .by_id
                .get(&table)
                .into_iter()
                .flatten()
                .filter(|(id, k)| s.spare_row(&table, **id, k) && pinned.is_none_or(|p| !p.contains_key(id)))
                .map(|(id, k)| (*id, k.clone()))
                .collect();
            let mut released: Vec<(i64, String)> = pinned
                .into_iter()
                .flatten()
                .filter(|(id, record)| {
                    s.spare_pin(&table, **id, record)
                        && !s.by_id.get(&table).is_some_and(|ids| ids.contains_key(*id))
                })
                .map(|(id, record)| (*id, record.clone()))
                .collect();
            released.sort();
            spare.extend(released);
            spare
        };
        let (id, from) = spare.into_iter().find(|(id, _)| free(*id))?;
        let mut s = self.store.borrow_mut();
        s.put(key, id, IdFrom::Reused);
        s.reused += 1;
        Some((id, from))
    }
}
