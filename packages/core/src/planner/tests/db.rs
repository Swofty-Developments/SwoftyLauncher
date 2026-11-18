use super::*;

#[test]
fn build_fields_apply_to_their_own_addon_only() {
    use swofty_formats::gvdb::Record;
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let add = r#"{"op":"db.add-row","table":"t","from":"t.r_0"}"#;
    let db = || {
        Gvdb::new(vec![
            Record::new("t._r_instancecount_", 0, "1"),
            Record::new("t.r_0.a", 0, "7"),
        ])
    };
    let mut p = Planner::new(&features, &opt_in);
    p.add_fields("a", BTreeMap::from([("t.r".to_owned(), vec!["a".to_owned()])]));
    p.push(op("a", add));
    let mut built = db();
    p.apply_db(&mut built).unwrap();
    assert_eq!(built.get_text("t.r_1.a").as_deref(), Some("7"));
    let mut p = Planner::new(&features, &opt_in);
    p.add_fields("a", BTreeMap::from([("t.r".to_owned(), vec!["a".to_owned()])]));
    p.push(op("b", add));
    let e = p.apply_db(&mut db()).unwrap_err();
    assert!(e.to_string().contains("field names unknown"), "{e}");
}

#[test]
fn a_stat_id_an_earlier_addon_pins_moves_and_counts_as_a_conflict() {
    use swofty_formats::gvdb::Record;
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let fields = || {
        BTreeMap::from([(
            "t.r".to_owned(),
            vec!["_metadata_".to_owned(), "stat_id".to_owned()],
        )])
    };
    let mut p = Planner::new(&features, &opt_in);
    p.add_fields("a", fields());
    p.add_fields("b", fields());
    p.push(op(
        "a",
        r#"{"op":"db.add-row","table":"t","from":"t.r_0","set":{"_metadata_":"a_row","stat_id":"6"}}"#,
    ));
    p.push(op(
        "b",
        r#"{"op":"db.add-row","table":"t","from":"t.r_0","set":{"_metadata_":"b_row","stat_id":"6"}}"#,
    ));
    let mut db = Gvdb::new(vec![
        Record::new("t._r_instancecount_", 0, "1"),
        Record::new("t.r_0._metadata_", 4, "base"),
        Record::new("t.r_0.stat_id", 0, "5"),
    ]);
    let stock = db.records.iter().map(|r| r.key).collect();
    let mut report = DbReport::default();
    p.apply_db_report(&mut db, &stock, &HashSet::new(), &mut report)
        .unwrap();
    assert_eq!(db.get_text("t.r_1.stat_id").as_deref(), Some("6"));
    assert_eq!(db.get_text("t.r_2.stat_id").as_deref(), Some("7"));
    assert!(
        report.warnings.iter().any(|w| w
            == "b: stat_id 6 of b_row is taken by a_row from a; the build gave it stat_id 7. The addon's author should leave stat_id out of add-row"),
        "{:?}",
        report.warnings
    );
    let planned = p.finish();
    assert_eq!(planned.conflicts.len(), 1);
    assert_eq!(
        planned.conflicts[0].target,
        Target::Record(swofty_formats::hash::fnv1a64("t.r_1.stat_id"))
    );
    assert_eq!(planned.conflicts[0].winner, origin("a"));
    assert_eq!(planned.conflicts[0].overridden, [origin("b")]);
    assert_eq!(
        planned.record_names.values().collect::<Vec<_>>(),
        ["t.r[_metadata_=a_row].stat_id"]
    );
    let (pairs, _) = crate::profile::conflicts::summarise("p", &planned.conflicts, &|_| None);
    assert_eq!(
        (
            pairs.pairs[0].winner.as_str(),
            pairs.pairs[0].loser.as_str(),
            pairs.pairs[0].database
        ),
        ("a", "b", 1)
    );
}

#[test]
fn a_free_stat_id_of_an_earlier_addon_leaves_the_one_a_later_addon_pins() {
    use swofty_formats::gvdb::Record;
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let fields = || {
        BTreeMap::from([(
            "t.r".to_owned(),
            vec!["_metadata_".to_owned(), "stat_id".to_owned()],
        )])
    };
    let mut p = Planner::new(&features, &opt_in);
    p.add_fields("a", fields());
    p.add_fields("b", fields());
    p.push(op(
        "a",
        r#"{"op":"db.add-row","table":"t","from":"t.r_0","set":{"_metadata_":"a_row"}}"#,
    ));
    p.push(op(
        "b",
        r#"{"op":"db.add-row","table":"t","from":"t.r_0","set":{"_metadata_":"b_row","stat_id":"6"}}"#,
    ));
    let mut db = Gvdb::new(vec![
        Record::new("t._r_instancecount_", 0, "1"),
        Record::new("t.r_0._metadata_", 4, "base"),
        Record::new("t.r_0.stat_id", 0, "5"),
    ]);
    let stock = db.records.iter().map(|r| r.key).collect();
    let mut report = DbReport::default();
    p.apply_db_report(&mut db, &stock, &HashSet::new(), &mut report)
        .unwrap();
    assert_eq!(db.get_text("t.r_1.stat_id").as_deref(), Some("7"));
    assert_eq!(db.get_text("t.r_2.stat_id").as_deref(), Some("6"));
    assert!(
        !report.warnings.iter().any(|w| w.contains("is taken by")),
        "{:?}",
        report.warnings
    );
    assert!(p.finish().conflicts.is_empty());
}

#[test]
fn a_package_stat_id_above_the_byte_ceiling_moves_with_a_warning() {
    use swofty_formats::gvdb::Record;
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let t = "entrants.vehicles.vehicle_packages";
    let mut p = Planner::new(&features, &opt_in);
    p.add_fields(
        "a",
        BTreeMap::from([(
            format!("{t}.vehicle_package"),
            vec!["_metadata_".to_owned(), "stat_id".to_owned()],
        )]),
    );
    p.push(op(
        "a",
        &format!(
            r#"{{"op":"db.add-row","table":"{t}","from":"{t}.vehicle_package_0","set":{{"_metadata_":"era","stat_id":"300"}}}}"#
        ),
    ));
    let mut db = Gvdb::new(vec![
        Record::new(&format!("{t}._vehicle_package_instancecount_"), 0, "1"),
        Record::new(&format!("{t}.vehicle_package_0._metadata_"), 4, "base"),
        Record::new(&format!("{t}.vehicle_package_0.stat_id"), 0, "58"),
    ]);
    let stock = db.records.iter().map(|r| r.key).collect();
    let mut report = DbReport::default();
    p.apply_db_report(&mut db, &stock, &HashSet::new(), &mut report)
        .unwrap();
    assert_eq!(
        db.get_text(&format!("{t}.vehicle_package_1.stat_id")).as_deref(),
        Some("59")
    );
    assert_eq!(
        report.warnings,
        [
            "a: stat_id 300 of era is above 254, the highest the game reads in entrants.vehicles.vehicle_packages.vehicle_package; the build gave it stat_id 59. The addon's author should leave stat_id out of add-row"
        ]
    );
    assert!(p.finish().conflicts.is_empty());
}

#[test]
fn a_scenario_stat_id_an_earlier_addon_holds_moves_and_counts_as_a_conflict() {
    use swofty_formats::gvdb::Record;
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let t = "game_setup.scenarios";
    let fields = || {
        BTreeMap::from([(
            format!("{t}.scenario"),
            vec!["_metadata_".to_owned(), "statId".to_owned()],
        )])
    };
    let mut p = Planner::new(&features, &opt_in);
    p.add_fields("cs", fields());
    p.add_fields("era", fields());
    p.add_fields("cs22", fields());
    let add = |label: &str, id: u32| {
        format!(
            r#"{{"op":"db.add-row","table":"{t}","from":"{t}.scenario[_metadata_=f1]","set":{{"_metadata_":"{label}","statId":"{id}"}}}}"#
        )
    };
    p.push(op("cs", &add("cs_f1_2021", 20)));
    p.push(op("era", &add("era_2012", 20)));
    p.push(op(
        "era",
        &format!(
            r#"{{"op":"db.set","select":"{t}.scenario[statId=20]","field":"_metadata_","value":"era_found"}}"#
        ),
    ));
    p.push(op("cs22", &add("cs_f1_2022", 21)));
    let mut db = Gvdb::new(vec![
        Record::new(&format!("{t}._scenario_instancecount_"), 0, "2"),
        Record::new(&format!("{t}.scenario_0._metadata_"), 4, "f1"),
        Record::new(&format!("{t}.scenario_0.statId"), 0, "14"),
        Record::new(&format!("{t}.scenario_1._metadata_"), 4, "f1_alt"),
        Record::new(&format!("{t}.scenario_1.statId"), 0, "14"),
    ]);
    let stock = db.records.iter().map(|r| r.key).collect();
    let mut report = DbReport::default();
    p.apply_db_report(&mut db, &stock, &HashSet::new(), &mut report)
        .unwrap();
    assert_eq!(
        db.get_text(&format!("{t}.scenario_2.statId")).as_deref(),
        Some("20")
    );
    assert_eq!(
        db.get_text(&format!("{t}.scenario_3.statId")).as_deref(),
        Some("22")
    );
    assert_eq!(
        db.get_text(&format!("{t}.scenario_4.statId")).as_deref(),
        Some("21")
    );
    assert_eq!(
        db.get_text(&format!("{t}.scenario_3._metadata_")).as_deref(),
        Some("era_found")
    );
    assert_eq!(
        report.warnings,
        [
            "era: statId 20 of era_2012 is taken by cs_f1_2021 from cs; the build gave it statId 22. The addon's author should give the row a statId no other addon uses"
        ]
    );
    let planned = p.finish();
    assert_eq!(planned.conflicts.len(), 1);
    assert_eq!(
        planned.conflicts[0].target,
        Target::Record(swofty_formats::hash::fnv1a64(format!("{t}.scenario_2.statId")))
    );
    assert_eq!(
        planned.record_names.values().collect::<Vec<_>>(),
        ["game_setup.scenarios.scenario[_metadata_=cs_f1_2021].statId"]
    );
}

#[test]
fn database_files_keep_their_schema_and_report_replaced_records() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    let empty: OpFile = serde_json::from_str(r#"{"schema_version":1,"ops":[]}"#).unwrap();
    p.push_file(&origin("a"), FileKind::Database, empty.clone());
    p.push_file(&origin("a"), FileKind::Assets, empty);
    assert_eq!(p.db.len(), 1);
    let key = "rules.x";
    p.push(op(
        "a",
        &format!(r#"{{"op":"db.put-record","key":"{key}","type":0,"value":"31"}}"#),
    ));
    p.push(op(
        "b",
        &format!(r#"{{"op":"db.put-record","key":"{key}","type":0,"value":"32"}}"#),
    ));
    assert_eq!(p.db.len(), 2);
    let mut db = Gvdb::default();
    p.apply_db(&mut db).unwrap();
    assert_eq!(db.get_text(key).as_deref(), Some("2"));
    let planned = p.finish();
    assert_eq!(planned.conflicts.len(), 1);
    assert_eq!(planned.conflicts[0].winner, origin("b"));
    assert_eq!(planned.conflicts[0].overridden, [origin("a")]);
}

#[test]
fn each_database_file_counts_the_owned_rows_once_after_the_first() {
    use swofty_formats::gvdb::Record;
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let folder = "unlockable_content.unlockable_items.unlockable_item_list_0.folder_0";
    let mut db = Gvdb::new(vec![
        Record::new(
            "unlockable_content.unlockable_items._unlockable_item_list_instancecount_",
            0,
            "1",
        ),
        Record::new(
            "unlockable_content.unlockable_items.unlockable_item_list_0._folder_instancecount_",
            0,
            "1",
        ),
        Record::new(&format!("{folder}._unlockable_se_livery_instancecount_"), 0, "1"),
        Record::new(&format!("{folder}.unlockable_se_livery_0._metadata_"), 0, "stock"),
        Record::new(&format!("{folder}.unlockable_se_livery_0.a"), 0, "7"),
    ]);
    let add = format!(r#"{{"op":"db.add-row","table":"{folder}","from":"{folder}.unlockable_se_livery_0"}}"#);
    let mut p = Planner::new(&features, &opt_in);
    for addon in ["a", "b", "c"] {
        p.add_fields(
            addon,
            BTreeMap::from([(format!("{folder}.unlockable_se_livery"), vec!["a".to_owned()])]),
        );
        p.push(op(addon, &add));
    }
    let before = crate::game_db::owned::COUNTED.with(std::cell::Cell::get);
    let added = p.apply_db(&mut db).unwrap();
    let counted = crate::game_db::owned::COUNTED.with(std::cell::Cell::get) - before;
    let got: Vec<(&str, std::ops::Range<usize>)> =
        added.iter().map(|a| (a.addon.as_str(), a.rows.clone())).collect();
    assert_eq!(got, [("a", 1..2), ("b", 2..3), ("c", 3..4)]);
    assert_eq!(counted, 4, "one count before the first file and one after each");
}

#[test]
fn restoring_a_later_addon_s_records_puts_a_removed_one_back_and_overwrites_the_others_in_place() {
    use swofty_formats::gvdb::Record;
    let rec = |path: &str, v: &str| Record::new(path, 4, v);
    let guarded = vec![(1, rec("t.r_0.b", "later b")), (3, rec("t.r_0.c", "later c"))];
    let mut db = Gvdb::new(vec![
        rec("t.r_0.a", "a"),
        rec("t.r_0.x", "x"),
        rec("t.r_0.c", "c"),
    ]);
    crate::planner::db::restore_later_records(&mut db, guarded);
    let texts: Vec<String> = db.records.iter().map(|r| r.text().into_owned()).collect();
    assert_eq!(texts, ["a", "later b", "x", "later c"]);
    assert_eq!(db.get_text("t.r_0.x").as_deref(), Some("x"));
}
