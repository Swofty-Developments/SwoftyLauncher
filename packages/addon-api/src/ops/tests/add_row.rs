use std::collections::{BTreeMap, HashSet};

use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

use super::*;
use crate::ops::*;

#[test]
fn add_row_appends_and_bumps_count() {
    let mut db = db();
    let ops = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"entrants.teams",
            "from":"entrants.teams.team[_metadata_=gm_cadillac_26]",
            "set":{"_metadata_":"team12_26","display_name_code":"T12"}}]}"#,
    );
    apply_db_ops(&mut db, &ops, &names()).unwrap();
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(3));
    assert_eq!(
        db.get_text("entrants.teams.team_2._metadata_").as_deref(),
        Some("team12_26")
    );
    assert_eq!(
        db.get_text("entrants.teams.team_2.stat_id").as_deref(),
        Some("102")
    );
    assert_eq!(
        db.get_text("entrants.teams.team_2.display_name_code").as_deref(),
        Some("T12")
    );
    let bytes = db.to_bytes();
    assert_eq!(Gvdb::parse(&bytes).unwrap().to_bytes(), bytes);
}

#[test]
fn add_row_reads_field_names_from_the_file() {
    let mut db = db();
    let ops = ops(
        r#"{"schema_version":1,"fields":{"entrants.teams.team":["_metadata_","stat_id"]},"ops":[
            {"op":"add-row","table":"entrants.teams","from":"entrants.teams.team[_metadata_=mclaren_26]",
            "stat_id":"keep","set":{"_metadata_":"team12_26"}}]}"#,
    );
    apply_db_ops(&mut db, &ops, &BTreeMap::new()).unwrap();
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(3));
    assert_eq!(
        db.get_text("entrants.teams.team_2.stat_id").as_deref(),
        Some("100")
    );
    assert!(db.get("entrants.teams.team_2.display_name_code").is_none());
}

#[test]
fn add_row_needs_every_field_on_the_source_and_a_free_row() {
    let mut db = db();
    let missing = ops(
        r#"{"schema_version":1,"fields":{"entrants.teams.team":["_metadata_","nope"]},"ops":[
            {"op":"add-row","table":"entrants.teams","from":"entrants.teams.team_0"}]}"#,
    );
    let err = apply_db_ops(&mut db, &missing, &BTreeMap::new()).unwrap_err();
    assert!(err.to_string().contains("has no nope"), "{err}");
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(2));
    db.records
        .push(Record::new("entrants.teams.team_2._metadata_", 4, "stale"));
    let taken = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"entrants.teams","from":"entrants.teams.team_0"}]}"#,
    );
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &taken, &names(), &mut report).unwrap();
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(4));
    assert_eq!(
        db.get_text("entrants.teams.team_2._metadata_").as_deref(),
        Some("stale")
    );
    assert!(db.get("entrants.teams.team_3._metadata_").is_some());
}

#[test]
fn add_row_adds_after_rows_another_addon_left_above_the_count() {
    let mut db = db();
    for i in [2, 3] {
        db.records.push(Record::new(
            &format!("entrants.teams.team_{i}._metadata_"),
            4,
            format!("other_{i}"),
        ));
    }
    let stock: HashSet<u64> = db
        .records
        .iter()
        .filter(|r| !r.text().starts_with("other_"))
        .map(|r| r.key)
        .collect();
    let add = ops(
        r#"{"schema_version":1,"fields":{"entrants.teams.team":["_metadata_","stat_id","display_name_code"]},
            "ops":[{"op":"add-row","table":"entrants.teams","from":"entrants.teams.team_0",
            "set":{"_metadata_":"mine"}}]}"#,
    );
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add, &stock, &mut report).unwrap();
    assert_eq!(
        db.get_text("entrants.teams.team_2._metadata_").as_deref(),
        Some("other_2")
    );
    assert_eq!(
        db.get_text("entrants.teams.team_3._metadata_").as_deref(),
        Some("other_3")
    );
    assert_eq!(
        db.get_text("entrants.teams.team_4._metadata_").as_deref(),
        Some("mine")
    );
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(5));
    assert!(
        report
            .lines
            .iter()
            .any(|l| l.contains("rows 2 to 3 already hold")),
        "{:?}",
        report.lines
    );
}

#[test]
fn add_row_reuses_a_stock_row_above_a_lowered_count() {
    let mut db = db();
    for (f, v) in [
        ("_metadata_", "stale"),
        ("stat_id", "7"),
        ("display_name_code", "OLD"),
    ] {
        db.records
            .push(Record::new(&format!("entrants.teams.team_2.{f}"), 2, v));
    }
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let add = ops(
        r#"{"schema_version":1,"fields":{"entrants.teams.team":["_metadata_","stat_id","display_name_code"]},
            "ops":[{"op":"add-row","table":"entrants.teams","from":"entrants.teams.team_0",
            "set":{"_metadata_":"new_26"}}]}"#,
    );
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add, &stock, &mut report).unwrap();
    let key = fnv1a64("entrants.teams.team_2._metadata_");
    let found: Vec<&Record> = db.records.iter().filter(|r| r.key == key).collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text(), "new_26");
    assert_eq!(found[0].tag, 4);
    let code = fnv1a64("entrants.teams.team_2.display_name_code");
    assert_eq!(db.records.iter().find(|r| r.key == code).unwrap().text(), "XXX");
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(3));
    assert!(
        report.lines.iter().any(|l| l.contains("reuses 3 stock key(s)")),
        "{:?}",
        report.lines
    );
}

#[test]
fn add_row_into_a_nested_table() {
    let mut db = track_db();
    let first = ops(r#"{"schema_version":1,
            "fields":{"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData":
              ["_metadata_","track","_TrackNameData_instancecount_","TrackNameData_0._metadata_","TrackNameData_0.title"]},
            "ops":[
            {"op":"add-row","table":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026]",
             "from":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=melbourne]",
             "set":{"_metadata_":"okayama","track":"okayama"}},
            {"op":"set","select":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=okayama]",
             "field":"TrackNameData_0.title","value":"lng_okayama"}]}"#);
    apply_db_ops(&mut db, &first, &BTreeMap::new()).unwrap();
    let set = format!("{SETS}.TrackDataSet_1");
    assert_eq!(db.instance_count(&set, "TrackData"), Some(3));
    assert_eq!(
        db.instance_count(&format!("{SETS}.TrackDataSet_0"), "TrackData"),
        Some(3)
    );
    for (field, want) in [
        ("_metadata_", "okayama"),
        ("track", "okayama"),
        ("TrackNameData_0.title", "lng_okayama"),
    ] {
        let got = db.get_text(&format!("{set}.TrackData_2.{field}"));
        assert_eq!(got.as_deref(), Some(want), "{field}");
    }
    let resolved_fields = first.fields.values().next().unwrap().clone();
    let again: DbOps = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "fields": { format!("{set}.TrackData"): resolved_fields },
        "ops": [{ "op": "add-row", "table": set,
            "from": "track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=okayama]",
            "set": { "_metadata_": "sepang", "track": "sepang" } }]
    }))
    .unwrap();
    apply_db_ops(&mut db, &again, &BTreeMap::new()).unwrap();
    assert_eq!(db.instance_count(&set, "TrackData"), Some(4));
    assert_eq!(
        db.get_text(&format!("{set}.TrackData_3.TrackNameData_0.title"))
            .as_deref(),
        Some("lng_okayama")
    );
    let wrong_table = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"track_data.game_setup.TrackDataSet[_metadata_=f1]",
            "from":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=melbourne]"}]}"#,
    );
    let err = apply_db_ops(&mut db, &wrong_table, &BTreeMap::new()).unwrap_err();
    assert!(err.to_string().contains("is not a row of"), "{err}");
}

#[test]
fn add_row_copies_every_key_with_nested_rows_and_counts() {
    let mut db = modes(["3", "7"]);
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add_mode(""), &catalogue(&[]), &mut report).unwrap();
    let get = |k: &str| db.get_text(&format!("modes.mode_2.{k}"));
    assert_eq!(db.instance_count("modes", "mode"), Some(3));
    assert_eq!(get("_metadata_").as_deref(), Some("f1_1990"));
    assert_eq!(get("player.driver.preloaded").as_deref(), Some("True"));
    assert_eq!(get("_Com_instancecount_").as_deref(), Some("1"));
    assert_eq!(get("Com_0.save_data.group").as_deref(), Some("career"));
    assert_eq!(get("Com_0._Inner_instancecount_").as_deref(), Some("2"));
    assert_eq!(get("Com_0.Inner_1.value").as_deref(), Some("0.75"));
    assert_eq!(get("Loose_1._metadata_").as_deref(), Some("b"));
    assert!(get("Loose_2._metadata_").is_none());
    assert_eq!(db.get("modes.mode_2.Com_0.Inner_0.value").unwrap().tag, 1);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(
        report.lines.iter().any(|l| l.ends_with("copied 11 key(s)")),
        "{:?}",
        report.lines
    );
    let bytes = db.to_bytes();
    assert_eq!(Gvdb::parse(&bytes).unwrap().to_bytes(), bytes);
}

#[test]
fn a_fields_list_names_extra_keys_and_the_build_reports_what_it_misses() {
    let listed = ["_metadata_", "stat_id"];
    let mut db = modes(["3", "7"]);
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add_mode(""), &catalogue(&listed), &mut report).unwrap();
    assert_eq!(
        db.get_text("modes.mode_2.Com_0.Inner_1.value").as_deref(),
        Some("0.75")
    );
    assert!(
        report
            .lines
            .iter()
            .any(|l| l.contains("copied 9 key(s) the fields list for modes.mode doesn't name")),
        "{:?}",
        report.lines
    );

    let mut db = modes(["3", "7"]);
    let mut report = DbReport::default();
    let legacy = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"modes","from":"modes.mode_0",
            "copy":"listed","set":{"_metadata_":"x"}}]}"#,
    );
    apply_db_ops_report(&mut db, &legacy, &catalogue(&listed), &mut report).unwrap();
    assert!(db.get("modes.mode_2.player.driver.preloaded").is_none());
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains(
            "misses 9 key(s) of the source row, not copied: Com_0.Inner_0.value, Com_0.Inner_1.value"
        ),
        "{}",
        report.warnings[0]
    );

    let strict = ops(
        r#"{"schema_version":1,"strict":true,"ops":[{"op":"add-row","table":"modes","from":"modes.mode_0",
            "copy":"listed","set":{"_metadata_":"x"}}]}"#,
    );
    let err = apply_db_ops(&mut modes(["3", "7"]), &strict, &catalogue(&listed)).unwrap_err();
    assert!(err.to_string().ends_with("(strict)"), "{err}");
}

#[test]
fn a_lowered_row_count_in_set_drops_those_rows_from_the_gap_check() {
    let listed = [
        "_metadata_",
        "stat_id",
        "player.driver.preloaded",
        "_Com_instancecount_",
    ];
    let lowered = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"modes","from":"modes.mode_0",
            "copy":"listed","set":{"_metadata_":"x","_Com_instancecount_":"0"}}]}"#,
    );
    let mut report = DbReport::default();
    let mut db = modes(["3", "7"]);
    apply_db_ops_report(&mut db, &lowered, &catalogue(&listed), &mut report).unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("misses 2 key(s)")
            && report.warnings[0].ends_with("Loose_0._metadata_, Loose_1._metadata_"),
        "{}",
        report.warnings[0]
    );
}

#[test]
fn a_row_shape_missing_from_the_catalogue_warns_or_fails_when_strict() {
    let mut names = catalogue(&["_metadata_", "stat_id"]);
    names.shapes.remove("modes.mode_N.Com_N");
    let mut report = DbReport::default();
    apply_db_ops_report(&mut modes(["3", "7"]), &add_mode(""), &names, &mut report).unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("the key catalogue has no entry for modes.mode_N.Com_N"),
        "{}",
        report.warnings[0]
    );
    let err = apply_db_ops(&mut modes(["3", "7"]), &add_mode(r#","strict":true"#), &names).unwrap_err();
    assert!(
        err.to_string().contains("no entry for modes.mode_N.Com_N"),
        "{err}"
    );
}

#[test]
fn row_shapes_number_every_row_segment() {
    assert_eq!(row_shape("a.b_3.c.d_12"), "a.b_N.c.d_N");
    assert_eq!(
        row_shape("canned_events.Per-race DNFs.Per-race DNF_5"),
        "canned_events.Per-race DNFs.Per-race DNF_N"
    );
    assert_eq!(row_shape("x_.y"), "x_.y");
}
