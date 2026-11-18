use std::collections::HashSet;

use swofty_formats::hash::{fnv1a64, fnv1a64_from};

use super::*;
use crate::ops::*;

#[test]
fn set_with_selector_and_value_from_hash_placeholder() {
    let mut db = db();
    let h = format!("{{{:016x}}}", fnv1a64("hidden.world_table_"));
    let json = format!(
        r#"{{"schema_version":1,"ops":[{{"op":"set","select":"{h}[_metadata_=f1_26]","field":"worldEri",
            "value_from":"{h}[_metadata_=myteam_26].worldEri"}}]}}"#
    );
    apply_db_ops(&mut db, &ops(&json), &names()).unwrap();
    let key = fnv1a64_from(fnv1a64("hidden.world_table_"), "0.worldEri");
    assert_eq!(db.get_key(key).unwrap().text(), "_f1_26_myteam.wep");
}

#[test]
fn put_and_remove_records_by_path_and_hash() {
    let mut db = db();
    let count = db.records.len();
    let code = fnv1a64("entrants.teams.team_0.display_name_code");
    let ops = ops(&format!(
        r#"{{"schema_version":1,"ops":[
            {{"op":"put-record","key":"{code:#018x}","type":4,"value":"4d434c"}},
            {{"op":"put-record","key":"entrants.teams.team_0.new_field","type":0,"value":"3132"}},
            {{"op":"remove-record","key":"entrants.teams.team_1.stat_id","allow_stock_removal":true}}]}}"#
    ));
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let replaced = apply_db_ops(&mut db, &ops, &stock).unwrap();
    assert_eq!(replaced[0], code);
    assert_eq!(replaced.len(), 3);
    let r = db.get("entrants.teams.team_0.display_name_code").unwrap();
    assert_eq!((r.tag, r.text().as_ref()), (4, "MCL"));
    assert_eq!(
        db.get_text("entrants.teams.team_0.new_field").as_deref(),
        Some("12")
    );
    assert!(db.get("entrants.teams.team_1.stat_id").is_none());
    assert_eq!(db.records.len(), count);
    for bad in [
        r#"{"op":"put-record","key":"0x12","type":0,"value":"00"}"#,
        r#"{"op":"put-record","key":"a.b","type":0,"value":"0"}"#,
    ] {
        let op: DbOp = serde_json::from_str(bad).unwrap();
        assert!(apply_db_op(&mut db, &op, &names()).is_err(), "{bad}");
    }
}

#[test]
fn a_record_removed_and_put_back_in_one_file_reads_as_the_record_put_back() {
    let mut db = db();
    let count = db.records.len();
    let code = "entrants.teams.team_1.display_name_code";
    let json = format!(
        r#"{{"schema_version":1,"ops":[
            {{"op":"remove-record","key":"{code}","allow_stock_removal":true}},
            {{"op":"put-record","key":"{code}","type":2,"value":"4d434c"}},
            {{"op":"set","select":"entrants.teams.team_0","field":"display_name_code","value_from":"{code}"}},
            {{"op":"put-record","key":"{code}","type":2,"value":"414243"}}]}}"#
    );
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    apply_db_ops(&mut db, &ops(&json), &stock).unwrap();
    assert_eq!(
        db.get_text("entrants.teams.team_0.display_name_code").as_deref(),
        Some("MCL")
    );
    assert_eq!(db.get_text(code).as_deref(), Some("ABC"));
    assert_eq!(db.records.len(), count);
    assert_eq!(db.records.iter().filter(|r| r.key == fnv1a64(code)).count(), 1);
}

#[test]
fn remove_record_twice_is_idempotent() {
    let key = "entrants.teams.team_1.stat_id";
    let json = format!(
        r#"{{"schema_version":1,"ops":[
            {{"op":"remove-record","key":"{key}"}},
            {{"op":"remove-record","key":"{key}"}}]}}"#
    );
    let mut lenient = db();
    let mut report = DbReport::default();
    let replaced = apply_db_ops_report(&mut lenient, &ops(&json), &names(), &mut report).unwrap();
    assert_eq!(replaced, [fnv1a64(key)]);
    assert!(lenient.get(key).is_none());
    assert_eq!(report.lines, [format!("{key}: already removed")]);

    let strict = format!(
        r#"{{"schema_version":1,"strict":true,"ops":[
            {{"op":"remove-record","key":"{key}"}},
            {{"op":"remove-record","key":"{key}"}}]}}"#
    );
    apply_db_ops(&mut db(), &ops(&strict), &names()).unwrap();
}

#[test]
fn remove_record_keeps_stock_records_unless_the_op_allows_it() {
    let mut db = db();
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let json = r#"{"schema_version":1,"ops":[
            {"op":"put-record","key":"entrants.teams.team_0.extra","type":0,"value":"31"},
            {"op":"put-record","key":"entrants.teams.team_1.extra","type":0,"value":"32"},
            {"op":"remove-record","key":"entrants.teams.team_0.stat_id"},
            {"op":"remove-record","key":"entrants.teams.team[_metadata_=gm_cadillac_26].stat_id"},
            {"op":"remove-record","key":"entrants.teams.team_0.extra"},
            {"op":"remove-record","key":"entrants.teams.team_1._metadata_","allow_stock_removal":true}]}"#;
    let mut report = DbReport::default();
    let replaced = apply_db_ops_report(&mut db, &ops(json), &stock, &mut report).unwrap();
    assert_eq!(report.stock_removals, 2);
    assert!(report.warnings.is_empty(), "{report:?}");
    let extra = fnv1a64("entrants.teams.team_0.extra");
    assert_eq!(
        replaced,
        [
            extra,
            fnv1a64("entrants.teams.team_1.extra"),
            extra,
            fnv1a64("entrants.teams.team_1._metadata_")
        ]
    );
    for kept in ["entrants.teams.team_0.stat_id", "entrants.teams.team_1.stat_id"] {
        assert!(db.get(kept).is_some(), "{kept}");
    }
    assert!(db.get("entrants.teams.team_0.extra").is_none());
    assert_eq!(db.get_text("entrants.teams.team_1.extra").as_deref(), Some("2"));
    assert!(db.get("entrants.teams.team_1._metadata_").is_none());

    let mut unguarded = self::db();
    apply_db_ops(&mut unguarded, &ops(json), &names()).unwrap();
    assert!(unguarded.get("entrants.teams.team_0.stat_id").is_none());
    let op: DbOp = serde_json::from_str(r#"{"op":"remove-record","key":"a.b"}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&op).unwrap(),
        r#"{"op":"remove-record","key":"a.b"}"#
    );
}

#[test]
fn reports_the_records_ops_change() {
    let mut db = track_db();
    let ops = ops(&format!(
        r#"{{"schema_version":1,"ops":[
            {{"op":"put-record","key":"{SETS}.TrackDataSet_1._metadata_","type":4,"value":"6631"}},
            {{"op":"put-record","key":"{SETS}.TrackDataSet_0._metadata_","type":4,"value":"6631"}},
            {{"op":"set","select":"{SETS}.TrackDataSet_0.TrackData_0","field":"track","value":"x"}},
            {{"op":"remove-record","key":"{SETS}.TrackDataSet_0.TrackData_1.track"}}]}}"#
    ));
    let mut report = DbReport::default();
    apply_db_ops_lenient(&mut db, &ops, &names(), &mut report);
    let rw = |path: &str, was: Option<&str>, now: Option<&str>| Rewrite {
        key: fnv1a64(format!("{SETS}.{path}")),
        was: was.map(String::from),
        now: now.map(String::from),
    };
    assert_eq!(
        report.rewrites,
        [
            rw("TrackDataSet_1._metadata_", Some("f1_2026"), Some("f1")),
            rw("TrackDataSet_0.TrackData_0.track", Some("melbourne"), Some("x")),
            rw("TrackDataSet_0.TrackData_1.track", Some("okayama"), None),
        ]
    );
}

#[test]
fn set_missing_key_fails_without_create() {
    let mut db = db();
    let ops = ops(
        r#"{"schema_version":1,"ops":[{"op":"set","select":"entrants.teams.team_0","field":"nope","value":"1"}]}"#,
    );
    assert!(apply_db_ops(&mut db, &ops, &names()).is_err());
}

#[test]
fn list_append_adds_missing_values_once_and_list_remove_takes_them_out() {
    let mut db = db();
    let append = |v: &str| {
        ops(&format!(
            r#"{{"schema_version":1,"ops":[{{"op":"list-append","select":"entrants.teams.team[_metadata_=mclaren_26]","field":"display_name_code","value":"{v}"}}]}}"#
        ))
    };
    let key = fnv1a64("entrants.teams.team_0.display_name_code");
    assert!(
        apply_db_ops(&mut db, &append("YYY"), &names())
            .unwrap()
            .is_empty()
    );
    apply_db_ops(&mut db, &append("ZZZ, XXX"), &names()).unwrap();
    apply_db_ops(&mut db, &append("YYY"), &names()).unwrap();
    assert_eq!(db.get_key(key).unwrap().text(), "XXX,YYY,ZZZ");
    let remove = ops(
        r#"{"schema_version":1,"ops":[{"op":"list-remove","select":"entrants.teams.team_0","field":"display_name_code","value":"XXX,ZZZ"}]}"#,
    );
    assert!(apply_db_ops(&mut db, &remove, &names()).unwrap().is_empty());
    assert_eq!(db.get_key(key).unwrap().text(), "YYY");
}

#[test]
fn list_ops_on_a_missing_key_fail() {
    let mut db = db();
    let ops = ops(
        r#"{"schema_version":1,"ops":[{"op":"list-append","select":"entrants.teams.team_0","field":"nope","value":"a"}]}"#,
    );
    assert!(apply_db_ops(&mut db, &ops, &names()).is_err());
}
