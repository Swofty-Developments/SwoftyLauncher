mod add_row;
mod bench;
mod op_files;
mod records;
mod selector_index;
mod selectors;
mod stat_ids;

use std::collections::{BTreeMap, HashMap};

use swofty_formats::gvdb::{Gvdb, Record, instance_count_path};
use swofty_formats::hash::{fnv1a64, fnv1a64_from};

use super::*;

pub(super) fn db() -> Gvdb {
    let mut records = vec![Record::new("entrants.teams._team_instancecount_", 0, "2")];
    for (i, label) in ["mclaren_26", "gm_cadillac_26"].iter().enumerate() {
        records.push(Record::new(
            &format!("entrants.teams.team_{i}._metadata_"),
            4,
            *label,
        ));
        records.push(Record::new(
            &format!("entrants.teams.team_{i}.stat_id"),
            0,
            (100 + i).to_string(),
        ));
        records.push(Record::new(
            &format!("entrants.teams.team_{i}.display_name_code"),
            2,
            "XXX",
        ));
    }
    let world = fnv1a64("hidden.world_table_");
    for (i, (label, eri)) in [("f1_26", "_f1_26.wep"), ("myteam_26", "_f1_26_myteam.wep")]
        .iter()
        .enumerate()
    {
        for (f, v) in [("_metadata_", *label), ("worldEri", *eri)] {
            let key = fnv1a64_from(world, format!("{i}.{f}"));
            records.push(Record {
                key,
                tag: 2,
                value: v.as_bytes().to_vec(),
            });
        }
    }
    Gvdb::new(records)
}

pub(super) fn names() -> BTreeMap<String, Vec<String>> {
    let fields = ["_metadata_", "stat_id", "display_name_code"]
        .map(String::from)
        .to_vec();
    BTreeMap::from([("entrants.teams.team".to_owned(), fields)])
}

pub(super) fn ops(json: &str) -> DbOps {
    serde_json::from_str(json).unwrap()
}

pub(super) const SETS: &str = "track_data.game_setup";

pub(super) fn track_db() -> Gvdb {
    let mut records = vec![Record::new(&instance_count_path(SETS, "TrackDataSet"), 0, "2")];
    for (i, (label, tracks)) in [
        ("f1", &["melbourne", "okayama", "okayama"][..]),
        ("f1_2026", &["melbourne", "suzuka"][..]),
    ]
    .into_iter()
    .enumerate()
    {
        let set = format!("{SETS}.TrackDataSet_{i}");
        records.push(Record::new(&format!("{set}._metadata_"), 4, label));
        records.push(Record::new(
            &instance_count_path(&set, "TrackData"),
            0,
            tracks.len().to_string(),
        ));
        for (j, track) in tracks.iter().enumerate() {
            let row = format!("{set}.TrackData_{j}");
            records.push(Record::new(&format!("{row}._metadata_"), 4, *track));
            records.push(Record::new(&format!("{row}.track"), 2, *track));
            records.push(Record::new(
                &format!("{row}._TrackNameData_instancecount_"),
                0,
                "1",
            ));
            records.push(Record::new(
                &format!("{row}.TrackNameData_0._metadata_"),
                4,
                "names",
            ));
            records.push(Record::new(
                &format!("{row}.TrackNameData_0.title"),
                4,
                format!("lng_{label}_{track}"),
            ));
        }
    }
    Gvdb::new(records)
}

pub(super) struct Catalogue {
    pub(super) fields: BTreeMap<String, Vec<String>>,
    pub(super) shapes: HashMap<String, Vec<String>>,
}

impl FieldNames for Catalogue {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>> {
        self.fields.fields(table, row)
    }

    fn row_keys(&self, shape: &str) -> Option<&[String]> {
        self.shapes.get(shape).map(Vec::as_slice)
    }
}

pub(super) fn catalogue(fields: &[&str]) -> Catalogue {
    let shape = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
    Catalogue {
        fields: match fields.is_empty() {
            true => BTreeMap::new(),
            false => BTreeMap::from([("modes.mode".to_owned(), shape(fields))]),
        },
        shapes: HashMap::from([
            (
                "modes.mode_N".to_owned(),
                shape(&[
                    "_metadata_",
                    "stat_id",
                    "player.driver.preloaded",
                    "_Com_instancecount_",
                    "+Com",
                    "+Loose",
                    "+gone",
                ]),
            ),
            (
                "modes.mode_N.Com_N".to_owned(),
                shape(&["_metadata_", "save_data.group", "_Inner_instancecount_", "+Inner"]),
            ),
            ("modes.mode_N.Com_N.Inner_N".to_owned(), shape(&["value"])),
            ("modes.mode_N.Loose_N".to_owned(), shape(&["_metadata_"])),
        ]),
    }
}

pub(super) fn modes(ids: [&str; 2]) -> Gvdb {
    let mut records = vec![Record::new("modes._mode_instancecount_", 0, "2")];
    for (i, (label, id)) in ["f1", "classic"].iter().zip(ids).enumerate() {
        let r = format!("modes.mode_{i}");
        records.push(Record::new(&format!("{r}._metadata_"), 4, *label));
        records.push(Record::new(&format!("{r}.stat_id"), 0, id));
        records.push(Record::new(&format!("{r}.player.driver.preloaded"), 5, "True"));
    }
    for (k, tag, v) in [
        ("modes.mode_0._Com_instancecount_", 0, "1"),
        ("modes.mode_0.Com_0._metadata_", 4, "ComCareer"),
        ("modes.mode_0.Com_0.save_data.group", 2, "career"),
        ("modes.mode_0.Com_0._Inner_instancecount_", 0, "2"),
        ("modes.mode_0.Com_0.Inner_0.value", 1, "0.5"),
        ("modes.mode_0.Com_0.Inner_1.value", 1, "0.75"),
        ("modes.mode_0.Loose_0._metadata_", 4, "a"),
        ("modes.mode_0.Loose_1._metadata_", 4, "b"),
    ] {
        records.push(Record::new(k, tag, v));
    }
    Gvdb::new(records)
}

pub(super) fn add_mode(extra: &str) -> DbOps {
    ops(&format!(
        r#"{{"schema_version":1{extra},"ops":[{{"op":"add-row","table":"modes",
            "from":"modes.mode[_metadata_=f1]","set":{{"_metadata_":"f1_1990"}}}}]}}"#
    ))
}

pub(super) fn table(path: &str, row: &str, ids: &[i64]) -> Gvdb {
    let mut records = vec![Record::new(
        &format!("{path}._{row}_instancecount_"),
        0,
        ids.len().to_string(),
    )];
    for (i, id) in ids.iter().enumerate() {
        let r = format!("{path}.{row}_{i}");
        records.push(Record::new(&format!("{r}._metadata_"), 4, format!("stock_{i}")));
        records.push(Record::new(&format!("{r}.stat_id"), 0, id.to_string()));
    }
    Gvdb::new(records)
}

pub(super) fn ids(db: &Gvdb, path: &str, row: &str) -> Vec<i64> {
    let n = db.instance_count(path, row).unwrap();
    (0..n)
        .map(|i| {
            db.get_text(&format!("{path}.{row}_{i}.stat_id"))
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}
