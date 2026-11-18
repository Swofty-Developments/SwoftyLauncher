//! A `[field=value]` selector into a table's rows, answered from that table's values of the field. They
//! are read by one pass over its rows, the pass the selector makes, and kept while the batch applies until
//! a write can change them: a write to a row's field drops that field's values, and a write to the
//! table's instance count or to a row's `_metadata_` drops every field's.

use std::collections::HashMap;

use swofty_formats::gvdb::instance_count_path;
use swofty_formats::hash::fnv1a64;

use super::{Base, Row, row_name};
use crate::ops::game_db::Db;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Table {
    Path(String),
    Hash(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Selected {
    table: Table,
    row: String,
    field: String,
}

/// The first row holding each value and how many rows hold it.
type Values = HashMap<String, (Base, usize)>;

#[derive(Default)]
pub(crate) struct Selectors {
    kept: HashMap<Selected, Values>,
    /// The kept values each record was read into.
    feeds: HashMap<u64, Vec<Selected>>,
}

impl Selectors {
    /// Drops the kept values a write to the record `key` can change.
    pub(crate) fn forget(&mut self, key: u64) {
        if let Some(fed) = self.feeds.remove(&key) {
            for s in fed {
                self.kept.remove(&s);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn kept(&self) -> usize {
        self.kept.len()
    }
}

/// The first of `table`'s rows named `row` whose `field` is `want`, and how many rows hold it. `count` is
/// the table's instance count: the rows read are the ones under it, or up to the first with no
/// `_metadata_` when there is none.
pub(crate) fn select(
    db: &Db,
    table: &Base,
    row: &str,
    field: &str,
    count: Option<usize>,
    want: &str,
) -> Option<(Base, usize)> {
    let id = Selected {
        table: match table {
            Base::Path(p) => Table::Path(p.clone()),
            Base::Hash(h) => Table::Hash(*h),
        },
        row: row.to_owned(),
        field: field.to_owned(),
    };
    let mut selectors = db.selectors.borrow_mut();
    if !selectors.kept.contains_key(&id) {
        let mut values = Values::new();
        let mut fed = Vec::new();
        if let Base::Path(t) = table
            && !row.is_empty()
        {
            fed.push(fnv1a64(instance_count_path(t, row)));
        }
        for idx in 0..count.unwrap_or(4096) {
            let candidate = Row {
                base: row_name(table, idx, row),
            };
            let metadata = candidate.field("_metadata_");
            fed.push(metadata);
            if db.get(metadata).is_none() {
                if count.is_none() {
                    break;
                }
                continue;
            }
            let key = candidate.field(field);
            fed.push(key);
            if let Some(value) = db.text(key) {
                values
                    .entry(value)
                    .and_modify(|(_, hits)| *hits += 1)
                    .or_insert((candidate.base, 1));
            }
        }
        for key in fed {
            let into = selectors.feeds.entry(key).or_default();
            if !into.contains(&id) {
                into.push(id.clone());
            }
        }
        selectors.kept.insert(id.clone(), values);
    }
    selectors.kept[&id].get(want).cloned()
}
