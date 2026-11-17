use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::{REGISTRY_TABLES, UNHELD_REGISTRIES};

// The ids the careers in `ids.json` use, by the table whose rows they are ids of.
pub(super) fn career_ids(doc: &Value) -> HashMap<String, HashSet<i64>> {
    let mut out: HashMap<String, HashSet<i64>> = HashMap::new();
    let Some(careers) = doc["careers"].as_object() else {
        return out;
    };
    for (registry, table) in REGISTRY_TABLES {
        let ids = out.entry((*table).to_owned()).or_default();
        if UNHELD_REGISTRIES.contains(registry) {
            continue;
        }
        for career in careers.values() {
            if let Some(records) = career[*registry].as_object() {
                ids.extend(
                    records
                        .values()
                        .filter_map(|e| e.as_i64().or_else(|| e["id"].as_i64())),
                );
            }
        }
    }
    out.retain(|_, ids| !ids.is_empty());
    out
}

// The ids `ids.json` holds and its careers use, as `(table, id, record)`: the table whose rows they are
// ids of and the `<addon>/<label>` record each is the id of. A registry's own ids come before its careers'.
fn registry_records(doc: &Value) -> Vec<(&'static str, i64, &str)> {
    let mut out = Vec::new();
    for (registry, table) in REGISTRY_TABLES {
        if let Some(records) = doc["ids"][*registry].as_object() {
            out.extend(records.iter().filter_map(|(record, e)| {
                e.as_i64()
                    .or_else(|| e["id"].as_i64())
                    .map(|id| (*table, id, record.as_str()))
            }));
        }
        if let Some(careers) = doc["careers"]
            .as_object()
            .filter(|_| !UNHELD_REGISTRIES.contains(registry))
        {
            for career in careers.values() {
                if let Some(records) = career[*registry].as_object() {
                    out.extend(
                        records
                            .iter()
                            .filter_map(|(record, e)| e.as_i64().map(|id| (*table, id, record.as_str()))),
                    );
                }
            }
        }
    }
    out
}

// The ids `ids.json` holds and its careers use, by the table whose rows they are ids of.
pub(super) fn registry_ids(doc: &Value) -> HashMap<String, HashMap<i64, String>> {
    let mut out: HashMap<String, HashMap<i64, String>> = HashMap::new();
    for (table, id, record) in registry_records(doc) {
        out.entry(table.to_owned())
            .or_default()
            .insert(id, record.to_owned());
    }
    out
}
