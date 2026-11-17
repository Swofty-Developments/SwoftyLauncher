use swofty_formats::gvdb::instance_count_path;
use swofty_formats::hash::fnv1a64;

use crate::Error;
use crate::ops::resolve::{Base, first_segment, row_name};

pub(super) const NO_ROW: &str = "no row matches";

/// Whether `e` is a selector that matched no row, which a row added later can fix.
pub fn no_row(e: &Error) -> bool {
    matches!(e, Error::Resolve(m) if m.contains(&format!(": {NO_ROW}")))
}

/// A selector segment of a path: it picks the row `{table}.{row}_{i}` whose `field` is `want`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub table: String,
    pub row: String,
    pub field: String,
    pub want: String,
}

impl Selector {
    /// The key of row `i`'s `field`.
    pub fn field_key(&self, i: usize) -> u64 {
        row_name(&Base::Path(self.table.clone()), i, &self.row).key(&format!(".{}", self.field))
    }

    /// The key of the table's row count; `None` for a table of numbered rows, which has none.
    pub fn count_key(&self) -> Option<u64> {
        (!self.row.is_empty()).then(|| fnv1a64(instance_count_path(&self.table, &self.row)))
    }
}

/// The selector segments of a [`no_row`] error's path, in order, ending with the one that matched no row.
/// A segment after another selector has its table from the error, so only the failing one comes after one.
pub fn no_row_selectors(e: &Error) -> Vec<Selector> {
    let Error::Resolve(m) = e else {
        return Vec::new();
    };
    let Some((upto, scope)) = m.split_once(&format!(": {NO_ROW}")) else {
        return Vec::new();
    };
    let upto = upto.rsplit(": ").next().unwrap_or(upto);
    let resolved = scope.strip_prefix(" in ").and_then(|s| s.rsplit_once('.'));
    let mut out = Vec::new();
    let mut plain = Some(String::new());
    let mut rest = Some(upto);
    while let Some(p) = rest {
        let Ok((seg, tail)) = first_segment(p) else {
            return Vec::new();
        };
        rest = tail;
        let Some((field, want)) = seg.select else {
            if let Some(path) = plain.as_mut() {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(seg.name);
            }
            continue;
        };
        let (table, row) = match (rest, resolved, plain.take()) {
            (None, Some((table, row)), _) => (table.to_owned(), row.to_owned()),
            (_, _, Some(path)) if path.is_empty() => (seg.name.to_owned(), String::new()),
            (_, _, Some(path)) => (path, seg.name.to_owned()),
            (_, _, None) => continue,
        };
        if table.starts_with('{') {
            continue;
        }
        out.push(Selector {
            table,
            row,
            field: field.to_owned(),
            want: want.to_owned(),
        });
    }
    out
}

/// Whether `e` is an `add-row` that found no free `stat_id` in the vehicle or team table, up to the highest the game reads.
pub fn ids_exhausted(e: &Error) -> bool {
    matches!(e, Error::Resolve(m)
        if m.contains(": entrants.vehicles has no free stat_id") || m.contains(": entrants.teams has no free stat_id"))
}
