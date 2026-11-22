//! Track layer overrides that name a missing track layer.
//!
//! A `content_sets.content_set_N.track_layer_override_N` row names two rows of `track_data.track_layers` by label.
//! After the track database loads, the game copies the `override` layer's world (`worldAssetGroup`, `worldEri`) onto
//! the `target` layer, for every game setup mode (0x1413e3120). It checks neither label: a `target` with no layer
//! row is written through a null layer, and an `override` with no layer row is read past the end of the layer
//! table. After every addon's database ops, [`drop_missing`] drops each override row whose `target` or `override`
//! names no layer row, with one line per row: the rows left move down in order and the count is lowered. The keys
//! past the new count stay in the database, where the game ignores them, so no stock record is removed. It never
//! fails the build: a content set or layer table it cannot read is left as it is.

use std::collections::{HashMap, HashSet};

use swofty_formats::gvdb::{Gvdb, Record, instance_count_path};
use swofty_formats::hash::fnv1a64;

const SETS: &str = "content_sets";
const SET_ROW: &str = "content_set";
const OVERRIDE_ROW: &str = "track_layer_override";
/// The fields of an override row, the catalogue's whole list (tools/gamedb/catalog/keys.tsv.gz).
const FIELDS: [&str; 3] = ["_metadata_", "target", "override"];
const LAYERS: &str = "track_data.track_layers";
const LAYER_ROW: &str = "TrackLayer";

fn index(db: &Gvdb) -> HashMap<u64, usize> {
    let mut at = HashMap::with_capacity(db.records.len());
    for (i, r) in db.records.iter().enumerate() {
        at.entry(r.key).or_insert(i);
    }
    at
}

fn text(db: &Gvdb, at: &HashMap<u64, usize>, path: &str) -> Option<String> {
    at.get(&fnv1a64(path))
        .map(|&i| db.records[i].text().trim().to_owned())
}

fn count(db: &Gvdb, at: &HashMap<u64, usize>, path: &str) -> Option<usize> {
    text(db, at, path).and_then(|t| t.parse().ok())
}

/// A record's tag and value bytes.
type Value = (u8, Vec<u8>);

struct Row {
    label: String,
    target: String,
    from: String,
    records: Vec<Option<Record>>,
}

fn missing<'a>(row: &'a Row, layers: &HashSet<String>) -> Option<(&'static str, &'a str)> {
    if !layers.contains(&row.target) {
        return Some(("target", &row.target));
    }
    if !layers.contains(&row.from) {
        return Some(("override", &row.from));
    }
    None
}

/// Drops the override rows that name a missing track layer; returns the build's report lines.
pub fn drop_missing(db: &mut Gvdb) -> Vec<String> {
    let mut lines = Vec::new();
    let at = index(db);
    let Some(sets) = count(db, &at, &instance_count_path(SETS, SET_ROW)) else {
        return lines;
    };
    let layers: Option<HashSet<String>> = count(db, &at, &instance_count_path(LAYERS, LAYER_ROW)).map(|n| {
        (0..n)
            .filter_map(|i| text(db, &at, &format!("{LAYERS}.{LAYER_ROW}_{i}._metadata_")))
            .filter(|l| !l.is_empty())
            .collect()
    });
    let mut writes: Vec<(u64, Option<Value>)> = Vec::new();
    for i in 0..sets {
        let base = format!("{SETS}.{SET_ROW}_{i}");
        let count_path = instance_count_path(&base, OVERRIDE_ROW);
        let Some(n) = count(db, &at, &count_path) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        let set = text(db, &at, &format!("{base}._metadata_")).unwrap_or_else(|| base.clone());
        let Some(layers) = &layers else {
            lines.push(format!(
                "content set {set}: {n} track_layer_override row(s) kept unchecked: the build cannot read the {LAYERS} count"
            ));
            continue;
        };
        let rows: Vec<Row> = (0..n)
            .map(|j| {
                let row = format!("{base}.{OVERRIDE_ROW}_{j}");
                let field = |f: &str| text(db, &at, &format!("{row}.{f}")).unwrap_or_default();
                Row {
                    label: text(db, &at, &format!("{row}._metadata_")).unwrap_or_else(|| row.clone()),
                    target: field("target"),
                    from: field("override"),
                    records: FIELDS
                        .iter()
                        .map(|f| {
                            at.get(&fnv1a64(format!("{row}.{f}")))
                                .map(|&k| db.records[k].clone())
                        })
                        .collect(),
                }
            })
            .collect();
        let mut kept = Vec::new();
        for row in &rows {
            match missing(row, layers) {
                Some((field, name)) => {
                    let name = if name.is_empty() { "(empty)" } else { name };
                    let effect = if field == "target" {
                        "the game writes through a null layer there"
                    } else {
                        "the game reads past the end of the layer table there"
                    };
                    lines.push(format!(
                        "content set {set}: dropped track_layer_override {} ({field} {name} names no {LAYERS} row; {effect})",
                        row.label
                    ));
                }
                None => kept.push(row),
            }
        }
        if kept.len() == rows.len() {
            continue;
        }
        for (j, row) in kept.iter().enumerate() {
            for (f, r) in FIELDS.iter().zip(&row.records) {
                let key = fnv1a64(format!("{base}.{OVERRIDE_ROW}_{j}.{f}"));
                writes.push((key, r.as_ref().map(|r| (r.tag, r.value.clone()))));
            }
        }
        let count_key = fnv1a64(&count_path);
        let tag = at.get(&count_key).map_or(0, |&k| db.records[k].tag);
        writes.push((count_key, Some((tag, kept.len().to_string().into_bytes()))));
    }
    let mut gone = HashSet::new();
    for (key, value) in writes {
        match (at.get(&key), value) {
            (Some(&i), Some((tag, value))) => {
                db.records[i].tag = tag;
                db.records[i].value = value;
            }
            (None, Some((tag, value))) => db.records.push(Record { key, tag, value }),
            (Some(_), None) => {
                gone.insert(key);
            }
            (None, None) => {}
        }
    }
    if !gone.is_empty() {
        db.records.retain(|r| !gone.contains(&r.key));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(path: &str, tag: u8, value: &str) -> Record {
        Record::new(path, tag, value)
    }

    fn get(db: &Gvdb, path: &str) -> Option<String> {
        text(db, &index(db), path)
    }

    fn layers(records: &mut Vec<Record>, labels: &[&str]) {
        records.push(rec(
            &instance_count_path(LAYERS, LAYER_ROW),
            0,
            &labels.len().to_string(),
        ));
        for (i, l) in labels.iter().enumerate() {
            records.push(rec(&format!("{LAYERS}.{LAYER_ROW}_{i}._metadata_"), 4, l));
        }
    }

    fn set(records: &mut Vec<Record>, i: usize, label: &str, overrides: &[(&str, &str, &str)]) {
        let base = format!("{SETS}.{SET_ROW}_{i}");
        records.push(rec(&format!("{base}._metadata_"), 4, label));
        records.push(rec(
            &instance_count_path(&base, OVERRIDE_ROW),
            0,
            &overrides.len().to_string(),
        ));
        for (j, (name, target, from)) in overrides.iter().enumerate() {
            let row = format!("{base}.{OVERRIDE_ROW}_{j}");
            records.push(rec(&format!("{row}._metadata_"), 4, name));
            records.push(rec(&format!("{row}.target"), 4, target));
            records.push(rec(&format!("{row}.override"), 4, from));
        }
    }

    fn row(db: &Gvdb, set: usize, j: usize) -> (String, String, String) {
        let base = format!("{SETS}.{SET_ROW}_{set}.{OVERRIDE_ROW}_{j}");
        let f = |f: &str| get(db, &format!("{base}.{f}")).unwrap_or_default();
        (f("_metadata_"), f("target"), f("override"))
    }

    fn owned(t: (&str, &str, &str)) -> (String, String, String) {
        (t.0.into(), t.1.into(), t.2.into())
    }

    #[test]
    fn the_stock_override_stays() {
        let mut records = vec![rec(&instance_count_path(SETS, SET_ROW), 0, "2")];
        layers(&mut records, &["f1", "f2", "classic", "f1_26"]);
        set(
            &mut records,
            0,
            "2025",
            &[("track_layer_override", "f1", "classic")],
        );
        set(&mut records, 1, "2026", &[]);
        let before = records.len();
        let mut db = Gvdb::new(records);
        let lines = drop_missing(&mut db);
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!(db.records.len(), before);
        assert_eq!(row(&db, 0, 0), owned(("track_layer_override", "f1", "classic")));
    }

    #[test]
    fn a_missing_target_or_override_is_dropped_and_the_rest_keep_their_order() {
        let mut records = vec![rec(&instance_count_path(SETS, SET_ROW), 0, "2")];
        layers(&mut records, &["f1", "classic", "f1_26"]);
        set(
            &mut records,
            0,
            "2025",
            &[
                ("a", "f1", "classic"),
                ("b", "gone", "classic"),
                ("c", "f1_26", "f1"),
                ("d", "f1", "nowhere"),
                ("e", "classic", "f1_26"),
            ],
        );
        set(&mut records, 1, "2026", &[("f", "f1", "")]);
        let before = records.len();
        let mut db = Gvdb::new(records);
        let lines = drop_missing(&mut db);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(
            lines[0].contains("content set 2025: dropped track_layer_override b (target gone names no"),
            "{lines:?}"
        );
        assert!(lines[0].contains("writes through a null layer"), "{lines:?}");
        assert!(
            lines[1].contains("dropped track_layer_override d (override nowhere names no"),
            "{lines:?}"
        );
        assert!(lines[1].contains("reads past the end"), "{lines:?}");
        assert!(
            lines[2].contains("content set 2026: dropped track_layer_override f (override (empty)"),
            "{lines:?}"
        );
        let count_of = |db: &Gvdb, i: usize| {
            get(
                db,
                &instance_count_path(&format!("{SETS}.{SET_ROW}_{i}"), OVERRIDE_ROW),
            )
        };
        assert_eq!(count_of(&db, 0).as_deref(), Some("3"));
        assert_eq!(count_of(&db, 1).as_deref(), Some("0"));
        assert_eq!(row(&db, 0, 0), owned(("a", "f1", "classic")));
        assert_eq!(row(&db, 0, 1), owned(("c", "f1_26", "f1")));
        assert_eq!(row(&db, 0, 2), owned(("e", "classic", "f1_26")));
        assert_eq!(
            row(&db, 0, 3),
            owned(("d", "f1", "nowhere")),
            "past the count, as it was"
        );
        assert_eq!(
            row(&db, 1, 0),
            owned(("f", "f1", "")),
            "past the count, as it was"
        );
        assert_eq!(db.records.len(), before, "no record removed or added");
        let keys: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
        assert_eq!(keys.len(), db.records.len(), "no key twice");
    }

    #[test]
    fn a_database_without_the_tables_is_left_alone() {
        let mut db = Gvdb::new(vec![rec("game_setup._season_instancecount_", 0, "4")]);
        assert!(drop_missing(&mut db).is_empty());
        assert_eq!(db.records.len(), 1);

        let mut records = vec![rec(&instance_count_path(SETS, SET_ROW), 0, "1")];
        set(&mut records, 0, "2025", &[("a", "gone", "classic")]);
        let before = records.len();
        let mut db = Gvdb::new(records);
        let lines = drop_missing(&mut db);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("kept unchecked"), "{lines:?}");
        assert_eq!(db.records.len(), before);
        assert_eq!(row(&db, 0, 0), owned(("a", "gone", "classic")));
    }
}
