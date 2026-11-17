use std::collections::HashSet;

use swofty_formats::gvdb::{Record, instance_count_path};
use swofty_formats::hash::{fnv1a64, fnv1a64_from};

use super::*;
use crate::Error;
use crate::ops::*;

fn set_track_op(select: &str, value: &str) -> DbOp {
    DbOp::Set {
        select: select.to_owned(),
        field: "track".into(),
        value: Some(value.into()),
        value_from: None,
        datasets: Vec::new(),
        create: false,
        tag: None,
    }
}

#[test]
fn duplicate_labels_resolve_to_the_earliest_row_with_a_warning() {
    let select = "track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[_metadata_=okayama]";
    let mut ops = ops(r#"{"schema_version": 1, "ops": []}"#);
    ops.ops = vec![set_track_op(select, "first"), set_track_op(select, "again")];
    let mut db = track_db();
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &ops, &names(), &mut report).unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].starts_with(&format!("{select}: 2 rows match; using the first")),
        "{:?}",
        report.warnings
    );
    let value = |row: usize| {
        let key = fnv1a64(format!("{SETS}.TrackDataSet_0.TrackData_{row}.track"));
        db.records
            .iter()
            .find(|r| r.key == key)
            .map(|r| String::from_utf8_lossy(&r.value).into_owned())
    };
    assert_eq!(value(1).as_deref(), Some("again"));
    assert_eq!(value(2).as_deref(), Some("okayama"));
}

#[test]
fn duplicate_labels_still_fail_where_the_choice_changes_the_result() {
    let key = "track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[_metadata_=okayama]._metadata_";
    let op = DbOp::RemoveRecord {
        key: key.into(),
        allow_stock_removal: true,
    };
    let err = apply_db_op(&mut track_db(), &op, &names())
        .unwrap_err()
        .to_string();
    assert!(
        err.ends_with("2 rows match in track_data.game_setup.TrackDataSet_0.TrackData"),
        "{err}"
    );
}

#[test]
fn parses_segments_like_pkg_paths() {
    let (seg, rest) = first_segment("TrackData[name=a.b$c].TrackNameData_0.title").unwrap();
    assert_eq!(
        (seg.name, seg.select, rest),
        (
            "TrackData",
            Some(("name", "a.b$c")),
            Some("TrackNameData_0.title")
        )
    );
    let (seg, rest) = first_segment("t[ k =x[1]").unwrap();
    assert_eq!((seg.select, rest), (Some(("k", "x[1")), None));
    let (seg, _) = first_segment("t[k=x]y]").unwrap();
    assert_eq!(seg.select, Some(("k", "x]y")));
    let (seg, rest) = first_segment("{446668bc25ba571c}[k=v].a").unwrap();
    assert_eq!((seg.name, rest), ("{446668bc25ba571c}", Some("a")));
    for bad in ["t[k=v", "t[kv].a", "t[k=v]a"] {
        assert!(first_segment(bad).is_err(), "{bad}");
    }
}

#[test]
fn selects_rows_on_every_segment() {
    let mut db = track_db();
    let ops = ops(r#"{"schema_version":1,"ops":[
            {"op":"set","select":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=suzuka]",
             "field":"TrackNameData_0.title","value":"lng_a"},
            {"op":"set","select":"track_data.game_setup.TrackDataSet_1.TrackData[_metadata_=melbourne]",
             "field":"TrackNameData_0.title","value":"lng_b"},
            {"op":"set","select":"track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData_0",
             "field":"TrackNameData[_metadata_=names].title","value":"lng_c"}]}"#);
    let replaced = apply_db_ops(&mut db, &ops, &names()).unwrap();
    let key = |p: &str| fnv1a64(format!("{SETS}.{p}.TrackNameData_0.title"));
    assert_eq!(
        replaced,
        [
            key("TrackDataSet_1.TrackData_1"),
            key("TrackDataSet_1.TrackData_0"),
            key("TrackDataSet_0.TrackData_0")
        ]
    );
    for (row, want) in [
        ("TrackDataSet_1.TrackData_1", "lng_a"),
        ("TrackDataSet_1.TrackData_0", "lng_b"),
        ("TrackDataSet_0.TrackData_0", "lng_c"),
    ] {
        let got = db.get_text(&format!("{SETS}.{row}.TrackNameData_0.title"));
        assert_eq!(got.as_deref(), Some(want), "{row}");
    }
}

#[test]
fn nested_selector_errors_name_the_segment() {
    let set = |select: &str| {
        let op = DbOp::Set {
            select: select.to_owned(),
            field: "track".into(),
            value: Some("x".into()),
            value_from: None,
            datasets: Vec::new(),
            create: false,
            tag: None,
        };
        apply_db_op(&mut track_db(), &op, &names())
            .unwrap_err()
            .to_string()
    };
    assert_eq!(
        set("track_data.game_setup.TrackDataSet[_metadata_=f9].TrackData[_metadata_=suzuka]"),
        "track_data.game_setup.TrackDataSet[_metadata_=f9]: no row matches"
    );
    assert_eq!(
        set("track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=okayama]"),
        "track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=okayama]: \
             no row matches in track_data.game_setup.TrackDataSet_1.TrackData"
    );
    assert!(
        set("track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[track]").contains("bad selector")
    );
    assert!(set("track_data.game_setup..TrackDataSet[_metadata_=f1]").contains("empty segment"));
}

#[test]
fn no_row_errors_give_their_selectors() {
    let fail = |select: &str| {
        let op = DbOp::Set {
            select: select.to_owned(),
            field: "track".into(),
            value: Some("x".into()),
            value_from: None,
            datasets: Vec::new(),
            create: false,
            tag: None,
        };
        no_row_selectors(&apply_db_op(&mut track_db(), &op, &names()).unwrap_err())
    };
    let sel = |table: &str, row: &str, want: &str| Selector {
        table: table.into(),
        row: row.into(),
        field: "_metadata_".into(),
        want: want.into(),
    };
    assert_eq!(
        fail("track_data.game_setup.TrackDataSet[_metadata_=f9].TrackData[_metadata_=suzuka]"),
        [sel(SETS, "TrackDataSet", "f9")]
    );
    assert_eq!(
        fail("track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=okayama]"),
        [
            sel(SETS, "TrackDataSet", "f1_2026"),
            sel(&format!("{SETS}.TrackDataSet_1"), "TrackData", "okayama")
        ]
    );
    let s = sel(SETS, "TrackDataSet", "f1_2026");
    assert_eq!(
        s.field_key(1),
        fnv1a64(format!("{SETS}.TrackDataSet_1._metadata_"))
    );
    assert_eq!(
        s.count_key(),
        Some(fnv1a64(instance_count_path(SETS, "TrackDataSet")))
    );
    assert!(no_row_selectors(&Error::Resolve("t.x: no such key".into())).is_empty());
}

#[test]
fn value_from_and_record_keys_take_nested_selectors() {
    let mut db = track_db();
    let ops = ops(r#"{"schema_version":1,"ops":[
            {"op":"set","select":"track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[_metadata_=melbourne]",
             "field":"TrackNameData_0.title",
             "value_from":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=suzuka].TrackNameData_0.title"},
            {"op":"put-record","key":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=suzuka].laps","type":0,"value":"3533"},
            {"op":"remove-record","key":"track_data.game_setup.TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=melbourne].track",
             "allow_stock_removal":true}]}"#);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    apply_db_ops(&mut db, &ops, &stock).unwrap();
    let get = |p: &str| db.get_text(&format!("{SETS}.{p}"));
    assert_eq!(
        get("TrackDataSet_0.TrackData_0.TrackNameData_0.title").as_deref(),
        Some("lng_f1_2026_suzuka")
    );
    assert_eq!(get("TrackDataSet_1.TrackData_1.laps").as_deref(), Some("53"));
    assert_eq!(get("TrackDataSet_1.TrackData_0.track"), None);
    assert_eq!(
        get("TrackDataSet_0.TrackData_0.track").as_deref(),
        Some("melbourne")
    );
}

#[test]
fn hash_placeholder_takes_nested_selectors() {
    let mut db = db();
    let world = fnv1a64("hidden.world_table_");
    for (i, (label, n)) in [("a", "6"), ("b", "7")].iter().enumerate() {
        for (f, v) in [("_metadata_", *label), ("n", *n)] {
            db.records.push(Record {
                key: fnv1a64_from(world, format!("1.slot_{i}.{f}")),
                tag: 0,
                value: v.as_bytes().to_vec(),
            });
        }
    }
    let h = format!("{{{world:016x}}}");
    let json = format!(
        r#"{{"schema_version":1,"ops":[{{"op":"set","select":"{h}[_metadata_=f1_26]","field":"worldEri",
            "value_from":"{h}[_metadata_=myteam_26].slot[_metadata_=b].n"}}]}}"#
    );
    apply_db_ops(&mut db, &ops(&json), &names()).unwrap();
    assert_eq!(db.get_key(fnv1a64_from(world, "0.worldEri")).unwrap().text(), "7");
    let json = format!(
        r#"{{"schema_version":1,"ops":[{{"op":"set","select":"{h}[_metadata_=myteam_26].slot[_metadata_=c]","field":"n","value":"1"}}]}}"#
    );
    let err = apply_db_ops(&mut db, &ops(&json), &names())
        .unwrap_err()
        .to_string();
    let row = format!("{{{:016x}}}", fnv1a64_from(world, "1"));
    assert!(
        err.ends_with(&format!("[_metadata_=c]: no row matches in {row}.slot")),
        "{err}"
    );
}
