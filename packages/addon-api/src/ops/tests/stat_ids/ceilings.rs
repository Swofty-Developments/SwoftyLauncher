use std::collections::HashSet;

use super::*;
use crate::ops::*;

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
