use std::collections::{BTreeMap, HashSet};

use crate::ops::db_ops::IdFrom;

pub trait FieldNames {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>>;

    fn row_keys(&self, shape: &str) -> Option<&[String]> {
        let _ = shape;
        None
    }

    /// Whether `key` is a record of the game's stock database.
    fn stock(&self, key: u64) -> bool {
        let _ = key;
        false
    }

    /// Whether `key` is a record a track's activation added to the database.
    fn track(&self, key: u64) -> bool {
        let _ = key;
        false
    }

    /// Whether an addon later in the build sets `id` on a row it adds to `table`'s rows named `row`.
    fn reserved(&self, table: &str, row: &str, id: i64) -> bool {
        let _ = (table, row, id);
        false
    }

    /// The install's kept ids for the rows `add-row` gives an id of its own; see [`RowIds`].
    fn row_ids(&self) -> Option<&dyn RowIds> {
        None
    }
}

/// The ids the install keeps for the rows `add-row` gives an id of its own, so a row gets the same
/// id at every build whatever else the build adds. A key names an addon's row: `<addon>/<table>.<row>/<label>`.
pub trait RowIds {
    /// The key of this addon's row labelled `label` in `table`'s rows named `row`, claimed for this
    /// build, and the id kept for it.
    fn claim(&self, table: &str, row: &str, label: &str) -> Claimed;

    /// Whether `id` of `table`'s rows named `row` is kept for a row other than `key`, or held by a
    /// record of the core's registry.
    fn taken(&self, table: &str, row: &str, key: &str, id: i64) -> bool;

    /// Keeps `id` for `key`, which it got as `from` says.
    fn keep(&self, table: &str, row: &str, key: &str, id: i64, from: IdFrom);

    /// The lowest id kept for a row of an addon this build leaves out that no record of the core's registry
    /// holds and `free` accepts, moved to `key`, and the key that held it.
    fn reclaim(&self, table: &str, row: &str, key: &str, free: &dyn Fn(i64) -> bool)
    -> Option<(i64, String)>;

    /// The record of the core's registry that `id` of `table`'s rows named `row` is kept for, when saves
    /// hold the table's ids and the record is another addon's with a row label other than `label`.
    fn registry_record(&self, _table: &str, _row: &str, _label: Option<&str>, _id: i64) -> Option<String> {
        None
    }
}

/// A row's claimed key, its kept id, and the id the profile's last build gave a row with its label.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Claimed {
    pub key: String,
    pub kept: Option<i64>,
    pub seed: Option<i64>,
}

/// Stock record keys and no field names.
impl FieldNames for HashSet<u64> {
    fn fields(&self, _: &str, _: &str) -> Option<Vec<String>> {
        None
    }

    fn stock(&self, key: u64) -> bool {
        self.contains(&key)
    }
}

pub const ANY_SHAPE: &str = "*";

pub fn row_shape(path: &str) -> String {
    path.split('.')
        .map(|seg| match seg.rsplit_once('_') {
            Some((word, idx)) if !idx.is_empty() && idx.bytes().all(|b| b.is_ascii_digit()) => {
                format!("{word}_N")
            }
            _ => seg.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

impl FieldNames for BTreeMap<String, Vec<String>> {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>> {
        self.get(&format!("{table}.{row}")).cloned()
    }
}

pub(crate) struct Schema<'a> {
    pub(crate) file: &'a BTreeMap<String, Vec<String>>,
    pub(crate) names: &'a dyn FieldNames,
}

impl FieldNames for Schema<'_> {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>> {
        self.file
            .fields(table, row)
            .or_else(|| self.names.fields(table, row))
    }

    fn row_keys(&self, shape: &str) -> Option<&[String]> {
        self.names.row_keys(shape)
    }

    fn stock(&self, key: u64) -> bool {
        self.names.stock(key)
    }

    fn track(&self, key: u64) -> bool {
        self.names.track(key)
    }

    fn reserved(&self, table: &str, row: &str, id: i64) -> bool {
        self.names.reserved(table, row, id)
    }

    fn row_ids(&self) -> Option<&dyn RowIds> {
        self.names.row_ids()
    }
}
