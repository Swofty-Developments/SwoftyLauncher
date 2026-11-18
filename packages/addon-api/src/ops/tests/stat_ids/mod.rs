mod cache;
mod ceilings;

use std::collections::HashSet;

use swofty_formats::gvdb::Gvdb;

use super::*;
use crate::ops::*;

#[test]
fn mask_tables_say_why_their_ceiling_is_31() {
    for (t, r) in [
        ("game_setup.game_setup_modes", "game_setup_mode"),
        ("entrants.entrant_setup_modes", "entrant_setup_mode"),
        ("game_setup.seasons", "season"),
    ] {
        assert_eq!(stat_id_ceiling(t, r).map(|c| c.max), Some(31), "{t}");
        assert!(
            stat_id_ceiling_reason(t, r).is_some_and(|w| w.contains("32-bit mask")),
            "{t}"
        );
    }
    assert_eq!(stat_id_ceiling_reason("entrants.vehicles", "vehicle"), None);
}

#[test]
fn a_third_car_group_fails_the_build_instead_of_the_main_menu() {
    const CARS: &str = "customisation.customisation_cars";
    let mut db = table(CARS, "customisation_car_group", &[0, 1]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let err = apply_db_ops_report(
        &mut db,
        &add_rows(CARS, "customisation_car_group", &[("sw12_cars", None)]),
        &stock,
        &mut DbReport::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("has no free stat_id up to 1"), "{err}");
    assert!(err.contains("crashes the game at the main menu"), "{err}");
}

#[test]
fn unlock_folders_name_their_list_and_share_its_stat_id_space() {
    let list = "unlockable_content.unlockable_items.unlockable_item_list_2";
    assert_eq!(unlock_list(&format!("{list}.folder_23")), Some(list));
    assert_eq!(unlock_list(list), None);
    assert_eq!(unlock_list(&format!("{list}.folder_x")), None);
    assert_eq!(unlock_list(&format!("{list}.folder_2.more")), None);
    assert_eq!(
        stat_id_field(&format!("{list}.folder_0"), "unlockable_suit"),
        "statId"
    );
    assert!(is_unlock_ids(&format!("{list}.{UNLOCK_ROW}")));
    assert!(!is_unlock_ids("entrants.teams.team"));
    let ops: DbOps = serde_json::from_str(&format!(
        r#"{{"schema_version":1,"ops":[
            {{"op":"add-row","table":"{list}.folder_23","from":"{list}.folder_23.unlockable_se_livery_0","set":{{"_metadata_":"a","statId":"7"}}}},
            {{"op":"add-row","table":"{list}.folder_1","from":"{list}.folder_1.unlockable_suit[_metadata_=x]","set":{{"_metadata_":"b"}}}}]}}"#
    ))
    .unwrap();
    assert_eq!(
        explicit_stat_ids(&ops).collect::<Vec<_>>(),
        [(format!("{list}.unlockable_item"), 7)]
    );
}

#[test]
fn add_row_gives_a_clone_the_next_free_stat_id_and_reports_it() {
    let mut db = modes(["3", "7"]);
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add_mode(""), &catalogue(&[]), &mut report).unwrap();
    apply_db_ops_report(&mut db, &add_mode(""), &catalogue(&[]), &mut report).unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("8"));
    assert_eq!(db.get_text("modes.mode_3.stat_id").as_deref(), Some("9"));
    assert!(
        report
            .lines
            .iter()
            .any(|l| l
                == "add-row modes.mode_2 (from modes.mode[_metadata_=f1]): stat_id 8 (the source's is 3)"),
        "{:?}",
        report.lines
    );
    let rows: Vec<(&str, &str)> = report
        .added_rows
        .iter()
        .map(|r| (r.from.as_str(), r.row.as_str()))
        .collect();
    assert_eq!(
        rows,
        [("modes.mode_0", "modes.mode_2"), ("modes.mode_0", "modes.mode_3")]
    );
}

#[test]
fn add_row_keeps_shared_stat_ids_and_explicit_ones() {
    let mut db = modes(["3", "3"]);
    apply_db_ops_report(&mut db, &add_mode(""), &catalogue(&[]), &mut DbReport::default()).unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("3"));

    let mut db = modes(["3", "7"]);
    let keep = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"modes","from":"modes.mode_0",
            "stat_id":"keep","set":{"_metadata_":"x"}},{"op":"add-row","table":"modes","from":"modes.mode_0",
            "set":{"_metadata_":"y","stat_id":"40"}}]}"#,
    );
    apply_db_ops_report(&mut db, &keep, &catalogue(&[]), &mut DbReport::default()).unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("3"));
    assert_eq!(db.get_text("modes.mode_3.stat_id").as_deref(), Some("40"));

    let mut db = modes(["3", "3"]);
    let fresh = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"modes","from":"modes.mode_0",
            "stat_id":"fresh","set":{"_metadata_":"x"}}]}"#,
    );
    apply_db_ops_report(&mut db, &fresh, &catalogue(&[]), &mut DbReport::default()).unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("4"));
}

struct Stocked {
    names: Catalogue,
    stock: HashSet<u64>,
}

impl FieldNames for Stocked {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>> {
        self.names.fields(table, row)
    }

    fn row_keys(&self, shape: &str) -> Option<&[String]> {
        self.names.row_keys(shape)
    }

    fn stock(&self, key: u64) -> bool {
        self.stock.contains(&key)
    }
}

fn stocked(db: &Gvdb) -> Stocked {
    Stocked {
        names: catalogue(&[]),
        stock: db.records.iter().map(|r| r.key).collect(),
    }
}

struct Reserved {
    inner: Stocked,
    ids: Vec<(String, i64)>,
}

impl FieldNames for Reserved {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>> {
        self.inner.fields(table, row)
    }

    fn row_keys(&self, shape: &str) -> Option<&[String]> {
        self.inner.row_keys(shape)
    }

    fn stock(&self, key: u64) -> bool {
        self.inner.stock(key)
    }

    fn reserved(&self, table: &str, row: &str, id: i64) -> bool {
        self.ids.contains(&(format!("{table}.{row}"), id))
    }
}

#[test]
fn a_free_stat_id_skips_the_id_a_later_addon_pins() {
    let mut db = modes(["3", "7"]);
    let names = Reserved {
        inner: stocked(&db),
        ids: vec![("modes.mode".to_owned(), 8)],
    };
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add_labelled("early", "", ""), &names, &mut report).unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("9"));
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_labelled("late", r#","stat_id":"8""#, ""),
        &names,
        &mut report,
    )
    .unwrap();
    assert_eq!(db.get_text("modes.mode_3.stat_id").as_deref(), Some("8"));
    assert!(report.moved_stat_ids.is_empty(), "{:?}", report.moved_stat_ids);
}

#[test]
fn two_pins_on_the_same_stat_id_still_conflict() {
    let mut db = modes(["3", "7"]);
    let names = Reserved {
        inner: stocked(&db),
        ids: vec![("modes.mode".to_owned(), 40)],
    };
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_labelled("a", r#","stat_id":"40""#, ""),
        &names,
        &mut report,
    )
    .unwrap();
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_labelled("b", r#","stat_id":"40""#, ""),
        &names,
        &mut report,
    )
    .unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("40"));
    assert_ne!(db.get_text("modes.mode_3.stat_id").as_deref(), Some("40"));
    assert_eq!(report.moved_stat_ids.len(), 1);
    assert_eq!(report.moved_stat_ids[0].old, 40);
}

fn add_labelled(label: &str, stat_id: &str, then: &str) -> DbOps {
    ops(&format!(
        r#"{{"schema_version":1,"ops":[{{"op":"add-row","table":"modes","from":"modes.mode[_metadata_=f1]",
            "set":{{"_metadata_":"{label}"{stat_id}}}}}{then}]}}"#
    ))
}

#[test]
fn an_explicit_stat_id_an_added_row_holds_gets_the_next_free_one() {
    let mut db = modes(["3", "7"]);
    let names = stocked(&db);
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &add_labelled("first", "", ""), &names, &mut report).unwrap();
    assert_eq!(db.get_text("modes.mode_2.stat_id").as_deref(), Some("8"));
    let mut report = DbReport::default();
    let ours = add_labelled(
        "second",
        r#","stat_id":"8""#,
        r#",{"op":"set","select":"modes.mode[stat_id=8]","field":"player.driver.preloaded","value":"False"},
            {"op":"set","select":"modes.mode_0","field":"player.driver.preloaded","value_from":"modes.mode[_metadata_=second].stat_id"}"#,
    );
    apply_db_ops_report(&mut db, &ours, &names, &mut report).unwrap();
    assert_eq!(db.get_text("modes.mode_3.stat_id").as_deref(), Some("9"));
    assert_eq!(
        report.moved_stat_ids,
        [MovedStatId {
            row: "modes.mode_3".into(),
            field: "stat_id",
            label: Some("second".into()),
            old: 8,
            new: 9,
            holder: Some("modes.mode_2".into()),
            holder_label: Some("first".into()),
            record: None,
        }]
    );
    assert!(
            report.lines.iter().any(|l| l
                == "add-row modes.mode_3 (from modes.mode[_metadata_=f1]): stat_id 8 is taken by modes.mode_2 (first); stat_id 9"),
            "{:?}",
            report.lines
        );
    assert_eq!(
        db.get_text("modes.mode_3.player.driver.preloaded").as_deref(),
        Some("False")
    );
    assert_eq!(
        db.get_text("modes.mode_2.player.driver.preloaded").as_deref(),
        Some("True")
    );
    assert_eq!(
        db.get_text("modes.mode_0.player.driver.preloaded").as_deref(),
        Some("9")
    );
}

#[test]
fn explicit_stat_ids_without_a_clash_or_shared_with_a_stock_row_stay() {
    for (id, want) in [("40", "40"), ("7", "7")] {
        let mut db = modes(["3", "7"]);
        let names = stocked(&db);
        apply_db_ops(&mut db, &add_labelled("first", "", ""), &names).unwrap();
        let mut report = DbReport::default();
        let ours = add_labelled("second", &format!(r#","stat_id":"{id}""#), "");
        apply_db_ops_report(&mut db, &ours, &names, &mut report).unwrap();
        assert_eq!(db.get_text("modes.mode_3.stat_id").as_deref(), Some(want));
        assert!(report.moved_stat_ids.is_empty(), "{:?}", report.moved_stat_ids);
        assert!(
            !report.lines.iter().any(|l| l.contains("is taken by")),
            "{:?}",
            report.lines
        );
    }
    let mut db = modes(["3", "7"]);
    let names = stocked(&db);
    let keep = ops(
        r#"{"schema_version":1,"ops":[{"op":"add-row","table":"modes","from":"modes.mode_0",
            "stat_id":"keep","set":{"_metadata_":"x"}},{"op":"add-row","table":"modes","from":"modes.mode_0",
            "set":{"_metadata_":"y","stat_id":"3"}}]}"#,
    );
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &keep, &names, &mut report).unwrap();
    assert_eq!(db.get_text("modes.mode_3.stat_id").as_deref(), Some("3"));
    assert!(report.moved_stat_ids.is_empty());
}

const PACKAGES: &str = "entrants.vehicles.vehicle_packages";

fn add_rows(path: &str, row: &str, rows: &[(&str, Option<i64>)]) -> DbOps {
    let ops: Vec<String> = rows
            .iter()
            .map(|(label, id)| {
                let id = id.map_or_else(String::new, |id| format!(r#","stat_id":"{id}""#));
                format!(
                    r#"{{"op":"add-row","table":"{path}","from":"{path}.{row}_0","set":{{"_metadata_":"{label}"{id}}}}}"#
                )
            })
            .collect();
    self::ops(&format!(
        r#"{{"schema_version":1,"fields":{{"{path}.{row}":["_metadata_","stat_id"]}},"ops":[{}]}}"#,
        ops.join(",")
    ))
}
