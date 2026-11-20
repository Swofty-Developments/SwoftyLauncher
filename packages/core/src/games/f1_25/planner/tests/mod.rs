use swofty_formats::gvdb::Gvdb;

use super::*;
use crate::asset::{FileKind, OpFile, Source};
use crate::ops::DbReport;

mod db;
mod routes;
mod skips;
mod waits;
mod writers;

fn origin(addon: &str) -> Origin {
    Origin {
        addon: addon.into(),
        item: None,
    }
}

fn op(addon: &str, json: &str) -> Op {
    Op {
        op: serde_json::from_str(json).unwrap(),
        origin: origin(addon),
    }
}

fn lists() -> Gvdb {
    use swofty_formats::gvdb::Record;
    Gvdb::new(vec![
        Record::new("t._list_instancecount_", 0, "1"),
        Record::new("t.list_0._metadata_", 4, "f1"),
        Record::new("t.list_0.name", 4, "stock"),
        Record::new("t.list_0.colour", 4, "red"),
    ])
}

fn set(row: &str, field: &str, value: &str) -> String {
    format!(r#"{{"op":"db.set","select":"t.list[_metadata_={row}]","field":"{field}","value":"{value}"}}"#)
}

fn add(label: &str) -> String {
    format!(
        r#"{{"op":"db.add-row","table":"t","from":"t.list[_metadata_=f1]","set":{{"_metadata_":"{label}","name":"{label}"}}}}"#
    )
}

fn list_fields(p: &mut Planner, addon: &str) {
    let names = ["_metadata_", "name", "colour"].map(str::to_owned).to_vec();
    p.add_fields(addon, BTreeMap::from([("t.list".to_owned(), names)]));
}

const SETS: &str = "track_data.game_setup";

fn track_sets() -> Gvdb {
    use swofty_formats::gvdb::Record;
    let mut records = vec![Record::new(
        &format!("{SETS}._TrackDataSet_instancecount_"),
        0,
        "3",
    )];
    for (i, label) in ["f1_2026", "f2_2023", "f2_2026"].into_iter().enumerate() {
        let set = format!("{SETS}.TrackDataSet_{i}");
        records.push(Record::new(&format!("{set}._metadata_"), 4, label));
        records.push(Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"));
        records.push(Record::new(
            &format!("{set}.TrackData_0._metadata_"),
            4,
            "bahrain",
        ));
        records.push(Record::new(&format!("{set}.TrackData_0.round"), 0, "1"));
    }
    Gvdb::new(records)
}

fn put(path: &str, value: &str) -> String {
    let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
    format!(r#"{{"op":"db.put-record","key":"{SETS}.{path}","type":4,"value":"{hex}"}}"#)
}

fn set_round(select: &str) -> String {
    format!(r#"{{"op":"db.set","select":"{SETS}.{select}","field":"round","value":"9"}}"#)
}

fn named(ids: &[(&str, &str)]) -> BTreeMap<String, String> {
    ids.iter()
        .map(|(id, name)| ((*id).to_owned(), (*name).to_owned()))
        .collect()
}

const SWAP: &str = "mod.serps-ineos-mercedes-concept";
const KOREA: &str = "pajko.korea";

fn swap_and_korea() -> BTreeMap<String, String> {
    named(&[(SWAP, "SERPS Ineos Mercedes Concept"), (KOREA, "Korea")])
}

const CAREER: &str = "career.classic-career";
