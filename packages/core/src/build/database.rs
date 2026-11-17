use std::collections::{BTreeMap, HashMap, HashSet};

use swofty_formats::gvdb::Gvdb;

use crate::multiplayer::fingerprint;

// Each addon's added rows of top-level tables with a stat id, as `<table>.<row>/<label>` to its id.
pub(super) fn added_row_ids(
    db: &Gvdb,
    copies: &[crate::dbedit::deps::Copy],
) -> BTreeMap<String, BTreeMap<String, i64>> {
    let rows: HashSet<&str> = copies.iter().map(|c| c.row.as_str()).collect();
    let mut wanted: HashMap<u64, (&str, &str, &str)> = HashMap::new();
    for c in copies {
        let Some((table, last)) = c.row.rsplit_once('.') else {
            continue;
        };
        let Some((row, _)) = last.rsplit_once('_') else {
            continue;
        };
        if crate::ops::row_shape(table).contains("_N") {
            continue;
        }
        let field = crate::ops::stat_id_field(table, row);
        wanted.insert(
            swofty_formats::hash::fnv1a64(format!("{}.{field}", c.row)),
            (&c.addon, &c.row, "id"),
        );
        wanted.insert(
            swofty_formats::hash::fnv1a64(format!("{}._metadata_", c.row)),
            (&c.addon, &c.row, "label"),
        );
    }
    let mut found: BTreeMap<&str, (&str, Option<String>, Option<i64>)> = BTreeMap::new();
    for r in &db.records {
        let Some(&(addon, row, what)) = wanted.get(&r.key) else {
            continue;
        };
        let e = found.entry(row).or_insert((addon, None, None));
        match what {
            "label" => e.1 = Some(r.text().into_owned()),
            _ => e.2 = r.text().trim().parse().ok(),
        }
    }
    let mut out: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for (row, (addon, label, id)) in found {
        let (Some(label), Some(id)) = (label, id) else {
            continue;
        };
        if !rows.contains(row) {
            continue;
        }
        let table = row.rsplit_once('_').map_or(row, |(t, _)| t);
        out.entry(addon.to_owned())
            .or_default()
            .insert(format!("{table}/{label}"), id);
    }
    out
}

pub(super) fn with_row_ids(
    mut outputs: BTreeMap<String, fingerprint::Output>,
    row_ids: &BTreeMap<String, BTreeMap<String, i64>>,
) -> BTreeMap<String, fingerprint::Output> {
    for (addon, ids) in row_ids {
        if let Some(o) = outputs.get_mut(addon) {
            o.row_ids = ids.clone();
        }
    }
    outputs
}
