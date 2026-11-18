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

#[test]
fn fresh_package_ids_past_the_byte_ceiling_take_the_lowest_free_ones_above_stock() {
    let mut db = table(PACKAGES, "vehicle_package", &[2, 5, 58]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let labels: Vec<String> = (0..90).map(|i| format!("pkg_{i}")).collect();
    let mut era: Vec<(&str, Option<i64>)> = labels[..24].iter().map(|l| (l.as_str(), None)).collect();
    era.push(("era_200", Some(200)));
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_rows(PACKAGES, "vehicle_package", &era),
        &stock,
        &mut report,
    )
    .unwrap();
    assert_eq!(
        ids(&db, PACKAGES, "vehicle_package")[3..],
        [(59..=82).collect::<Vec<_>>(), vec![200]].concat()
    );
    assert!(report.moved_stat_ids.is_empty());
    let more: Vec<(&str, Option<i64>)> = labels.iter().map(|l| (l.as_str(), None)).collect();
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_rows(PACKAGES, "vehicle_package", &more),
        &stock,
        &mut report,
    )
    .unwrap();
    let all = ids(&db, PACKAGES, "vehicle_package");
    let added = &all[3 + 25..];
    assert_eq!(added[..54], (201..=254).collect::<Vec<_>>());
    assert_eq!(added[54..], (83..=118).collect::<Vec<_>>());
    assert!(all.iter().all(|id| *id <= 254));
    assert_eq!(all.iter().collect::<HashSet<_>>().len(), all.len());
    assert!(
            report.lines.iter().any(|l| l
                == "add-row entrants.vehicles.vehicle_packages.vehicle_package_82 (from entrants.vehicles.vehicle_packages.vehicle_package_0): stat_id 83 (the source's is 2; 255 is above 254, the highest the game reads)"),
            "{:?}",
            report.lines
        );
}

#[test]
fn a_virtual_entrant_list_passes_the_byte_ceiling_its_row_never_reaches() {
    const LISTS: &str = "entrants.entrant_lists";
    let mut db = table(LISTS, "entrant_list", &[0, 255]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let full = apply_db_ops_report(
        &mut db.clone(),
        &add_rows(LISTS, "entrant_list", &[("season_1960", None)]),
        &stock,
        &mut DbReport::default(),
    )
    .unwrap_err();
    assert!(
        full.to_string().contains("has no free stat_id up to 255"),
        "{full}"
    );
    let mut ops = add_rows(LISTS, "entrant_list", &[("season_1960", None)]);
    ops.ops.insert(
        0,
        DbOp::VirtualList {
            list: "season_1960".into(),
        },
    );
    apply_db_ops_report(&mut db, &ops, &stock, &mut DbReport::default()).unwrap();
    assert_eq!(db.get_text(VIRTUAL_LISTS).as_deref(), Some("season_1960"));
    assert_eq!(
        db.get_text("entrants.entrant_lists.entrant_list_2._metadata_")
            .as_deref(),
        Some("season_1960")
    );
    assert_eq!(ids(&db, LISTS, "entrant_list"), [0, 255, 256]);
}

#[test]
fn leaderboard_descriptor_ids_stay_in_the_byte_the_game_reads() {
    const BOARDS: &str = "leaderboards";
    assert_eq!(
        stat_id_ceiling(BOARDS, "leaderboard_descriptor"),
        Some(StatIdCeiling { max: 255, from: None })
    );
    let mut db = table(BOARDS, "leaderboard_descriptor", &[0, 1, 3, 2, 5]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let rows = [("swofty_a", None), ("swofty_b", None), ("swofty_c", Some(300))];
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_rows(BOARDS, "leaderboard_descriptor", &rows),
        &stock,
        &mut report,
    )
    .unwrap();
    assert_eq!(
        ids(&db, BOARDS, "leaderboard_descriptor"),
        [0, 1, 3, 2, 5, 6, 7, 8]
    );
    assert_eq!(
        db.get_text("leaderboards.leaderboard_descriptor_5._metadata_")
            .as_deref(),
        Some("swofty_a")
    );
    let labels: Vec<String> = (0..260).map(|i| format!("box_{i}")).collect();
    let many: Vec<(&str, Option<i64>)> = labels.iter().map(|l| (l.as_str(), None)).collect();
    let mut full = table(BOARDS, "leaderboard_descriptor", &[0, 1, 3, 2, 5]);
    let stock: HashSet<u64> = full.records.iter().map(|r| r.key).collect();
    let err = apply_db_ops_report(
        &mut full,
        &add_rows(BOARDS, "leaderboard_descriptor", &many),
        &stock,
        &mut DbReport::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("has no free stat_id up to 255"), "{err}");
}

#[test]
fn package_ids_under_the_ceiling_stay_as_they_were() {
    let mut db = table(PACKAGES, "vehicle_package", &[2, 5, 58]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let rows = [("a", None), ("b", Some(120)), ("c", None), ("d", Some(254))];
    let mut report = DbReport::default();
    apply_db_ops_report(
        &mut db,
        &add_rows(PACKAGES, "vehicle_package", &rows),
        &stock,
        &mut report,
    )
    .unwrap();
    assert_eq!(
        ids(&db, PACKAGES, "vehicle_package"),
        [2, 5, 58, 59, 120, 121, 254]
    );
    assert!(report.moved_stat_ids.is_empty());
    assert!(
        !report
            .lines
            .iter()
            .any(|l| l.contains("the highest the game reads")),
        "{:?}",
        report.lines
    );
    let mut teams = table("entrants.teams", "team", &[0, 499]);
    let stock: HashSet<u64> = teams.records.iter().map(|r| r.key).collect();
    apply_db_ops(
        &mut teams,
        &add_rows("entrants.teams", "team", &[("t", None), ("u", Some(900))]),
        &stock,
    )
    .unwrap();
    assert_eq!(ids(&teams, "entrants.teams", "team"), [0, 499, 500, 900]);
}

#[test]
fn an_explicit_package_id_above_the_ceiling_moves_and_selectors_follow() {
    let mut db = table(PACKAGES, "vehicle_package", &[2, 5, 58]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let mut file = add_rows(PACKAGES, "vehicle_package", &[("high", Some(300))]);
    file.ops.extend(
            self::ops(&format!(
                r#"{{"schema_version":1,"ops":[{{"op":"set","select":"{PACKAGES}.vehicle_package[stat_id=300]","field":"_metadata_","value":"found"}}]}}"#
            ))
            .ops,
        );
    let mut report = DbReport::default();
    apply_db_ops_report(&mut db, &file, &stock, &mut report).unwrap();
    assert_eq!(ids(&db, PACKAGES, "vehicle_package"), [2, 5, 58, 59]);
    assert_eq!(
        db.get_text(&format!("{PACKAGES}.vehicle_package_3._metadata_"))
            .as_deref(),
        Some("found")
    );
    assert_eq!(
        report.moved_stat_ids,
        [MovedStatId {
            row: format!("{PACKAGES}.vehicle_package_3"),
            field: "stat_id",
            label: Some("high".into()),
            old: 300,
            new: 59,
            holder: None,
            holder_label: None,
            record: None,
        }]
    );
}

#[test]
fn driver_ids_fill_208_to_253_then_256_up_and_skip_254_255_and_low_byte_0xff() {
    let mut db = table("entrants.drivers", "driver", &[254, 0, 207, 210]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let labels: Vec<String> = (0..300).map(|i| format!("d_{i}")).collect();
    let rows: Vec<(&str, Option<i64>)> = labels.iter().map(|l| (l.as_str(), None)).collect();
    apply_db_ops(&mut db, &add_rows("entrants.drivers", "driver", &rows), &stock).unwrap();
    let all = ids(&db, "entrants.drivers", "driver");
    assert_eq!(all[4..7], [208, 209, 211]);
    let added = &all[4..];
    assert_eq!(
        added[..45],
        (208..=253).filter(|id| *id != 210).collect::<Vec<_>>()[..45]
    );
    assert_eq!(added[45], 256);
    assert_eq!(added[300 - 1], 510);
    assert!(
        added
            .iter()
            .all(|id| *id != 254 && *id != 255 && id & 0xff != 0xff && *id <= 16382)
    );
    assert_eq!(added.iter().filter(|id| **id > 255).count(), 300 - 45);
}

#[test]
fn driver_ids_stop_at_the_14_bit_ceiling() {
    let mut db = table("entrants.drivers", "driver", &[254]);
    let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let usable = (208..=16382)
        .filter(|id| !stat_id_unusable("entrants.drivers", "driver", *id))
        .count();
    let labels: Vec<String> = (0..usable).map(|i| format!("d_{i}")).collect();
    let rows: Vec<(&str, Option<i64>)> = labels.iter().map(|l| (l.as_str(), None)).collect();
    apply_db_ops(&mut db, &add_rows("entrants.drivers", "driver", &rows), &stock).unwrap();
    let all = ids(&db, "entrants.drivers", "driver");
    assert_eq!(*all.last().unwrap(), 16382);
    let e = apply_db_ops(
        &mut db,
        &add_rows("entrants.drivers", "driver", &[("full", None)]),
        &stock,
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("entrants.drivers has no free stat_id up to 16382"),
        "{e}"
    );
    let (_, failed) = apply_db_ops_lenient(
        &mut db,
        &add_rows("entrants.drivers", "driver", &[("full", None)]),
        &stock,
        &mut DbReport::default(),
    );
    assert_eq!(failed.len(), 1);
    assert_eq!(db.instance_count("entrants.drivers", "driver"), Some(1 + usable));
}

// The ids fresh rows took before 1.7.1, when each add-row read every row's id again: the next above
// the table's highest, or for a table with a ceiling the lowest free one from its `from` (or above
// the stock rows) that the game reads. None once nothing is free.
fn ids_before_the_cache(stock: &[i64], path: &str, row: &str, adds: usize) -> Vec<Option<i64>> {
    let ceiling = stat_id_ceiling(path, row);
    let stock_max = stock.iter().copied().max().unwrap_or(-1);
    let mut all = stock.to_vec();
    (0..adds)
        .map(|_| {
            let mut top = all.iter().copied().max().map_or(0, |m| m + 1);
            while stat_id_unusable(path, row, top) {
                top += 1;
            }
            let id = match ceiling {
                Some(c) if top > c.max || c.from.is_some() => {
                    let used: HashSet<i64> = all.iter().copied().collect();
                    (c.from.unwrap_or(stock_max + 1)..=c.max)
                        .find(|id| !used.contains(id) && !stat_id_unusable(path, row, *id))
                }
                _ => Some(top),
            };
            all.extend(id);
            id
        })
        .collect()
}

fn added_ids(stock: &[i64], path: &str, row: &str, adds: usize) -> Vec<Option<i64>> {
    let mut db = table(path, row, stock);
    let names: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let labels: Vec<String> = (0..adds).map(|i| format!("added_{i}")).collect();
    let rows: Vec<(&str, Option<i64>)> = labels.iter().map(|l| (l.as_str(), None)).collect();
    let (_, failed) = apply_db_ops_lenient(
        &mut db,
        &add_rows(path, row, &rows),
        &names,
        &mut DbReport::default(),
    );
    let mut got: Vec<Option<i64>> = ids(&db, path, row)[stock.len()..]
        .iter()
        .copied()
        .map(Some)
        .collect();
    got.extend(failed.iter().map(|_| None));
    got
}

#[test]
fn fresh_driver_ids_match_the_search_before_the_cache() {
    let stock = [1, 7, 30, 207, 254];
    assert_eq!(
        added_ids(&stock, "entrants.drivers", "driver", 400),
        ids_before_the_cache(&stock, "entrants.drivers", "driver", 400)
    );
}

#[test]
fn fresh_driver_ids_up_to_the_14_bit_ceiling_match_the_search_before_the_cache() {
    let mut stock: Vec<i64> = (208..=16100)
        .filter(|id| !stat_id_unusable("entrants.drivers", "driver", *id) && id % 97 != 0)
        .collect();
    stock.push(254);
    let before = ids_before_the_cache(&stock, "entrants.drivers", "driver", 450);
    assert_eq!(before.iter().flatten().max(), Some(&16382));
    assert!(before.last().unwrap().is_none());
    assert_eq!(added_ids(&stock, "entrants.drivers", "driver", 450), before);
}

#[test]
fn fresh_package_ids_past_the_byte_ceiling_match_the_search_before_the_cache() {
    let stock = [2, 5, 58];
    let before = ids_before_the_cache(&stock, PACKAGES, "vehicle_package", 300);
    assert!(before.last().unwrap().is_none());
    assert_eq!(added_ids(&stock, PACKAGES, "vehicle_package", 300), before);
}

#[test]
fn an_id_set_between_add_rows_frees_the_one_it_replaced() {
    let mut db = table("entrants.drivers", "driver", &[1, 7]);
    let names: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let ops = self::ops(
        r#"{"schema_version":1,"fields":{"entrants.drivers.driver":["_metadata_","stat_id"]},"ops":[
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"a"}},
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"b"}},
            {"op":"set","select":"entrants.drivers.driver_2","field":"stat_id","value":"300"},
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"c"}},
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"d"}}]}"#,
    );
    apply_db_ops(&mut db, &ops, &names).unwrap();
    assert_eq!(ids(&db, "entrants.drivers", "driver"), [1, 7, 300, 209, 208, 210]);
}
