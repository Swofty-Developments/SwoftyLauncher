use std::collections::HashMap;

use swofty_formats::gvdb::{Gvdb, Record, ValueType};

/// The shipped records `db` holds with other values: what the tracks changed.
pub(crate) fn changed_records(db: &Gvdb, shipped: &Gvdb) -> Vec<Record> {
    let stock: HashMap<u64, &[u8]> = shipped
        .records
        .iter()
        .map(|r| (r.key, r.value.as_slice()))
        .collect();
    db.records
        .iter()
        .filter(|r| stock.get(&r.key).is_some_and(|v| *v != r.value.as_slice()))
        .cloned()
        .collect()
}

/// Puts back the track changes in `changed` that database ops undid. A mod
/// made from a whole game database carries the shipped value of every record
/// it holds, so a record an op set back to its shipped value gets the tracks'
/// value again, and a list an op rewrote otherwise gets back the entries the
/// tracks added to it. Returns the keys put back.
pub(crate) fn keep_changes(db: &mut Gvdb, shipped: &Gvdb, changed: &[Record]) -> Vec<u64> {
    let mut kept = Vec::new();
    for ours in changed {
        let (Some(now), Some(stock)) = (db.get_key(ours.key), shipped.get_key(ours.key)) else {
            continue;
        };
        let value = if now.value == ours.value {
            None
        } else if now.value == stock.value {
            Some(ours.value.clone())
        } else if now.value_type() == ValueType::List && ours.value_type() == ValueType::List {
            merged_list(&now.value, &ours.value, &stock.value)
        } else {
            None
        };
        if let Some(value) = value
            && let Some((_, now)) = db.records.value_mut(ours.key)
        {
            *now = value;
            kept.push(ours.key);
        }
    }
    kept
}

fn merged_list(now: &[u8], ours: &[u8], stock: &[u8]) -> Option<Vec<u8>> {
    let items = |v: &[u8]| -> Vec<String> {
        String::from_utf8_lossy(v)
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let (mut out, shipped) = (items(now), items(stock));
    let added: Vec<String> = items(ours)
        .into_iter()
        .filter(|c| !shipped.contains(c) && !out.contains(c))
        .collect();
    if added.is_empty() {
        return None;
    }
    out.extend(added);
    Some(out.join(",").into_bytes())
}
