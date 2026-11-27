use super::*;

fn record_track_game(root: &Path) -> Game {
    use swofty_formats::gvdb::Record;
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    for s in &SLOTS[..4] {
        let slot = dir.join(slot_file(s));
        std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
        std::fs::write(&slot, archive(&["eaid://ui_package/slot.uil"], 0)).unwrap();
    }
    let res = "tracks.track_resource.TrackResource_0";
    let set = "track_data.game_setup.TrackDataSet_0";
    let layer = "track_data.track_layers.TrackLayer_0";
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "2"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.statId", 0, "5"),
        Record::new("tracks.Track_0.track_resource", 2, "2019"),
        Record::new("tracks.Track_1._metadata_", 2, "sochi"),
        Record::new("tracks.Track_1.statId", 0, "9"),
        Record::new("tracks.Track_1.track_resource", 2, "2019"),
        Record::new("tracks.track_resource._TrackResource_instancecount_", 0, "1"),
        Record::new(&format!("{res}._metadata_"), 2, "2019"),
        Record::new(&format!("{res}.iblAssetGroupsRoot"), 2, "probes/"),
        Record::new(&format!("{res}.iblKey"), 2, "key_2019.assetgroup"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{set}._metadata_"), 2, "f1"),
        Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"),
        Record::new(&format!("{set}.TrackData_0._metadata_"), 2, "melbourne"),
        Record::new(&format!("{set}.TrackData_0.track"), 2, "melbourne"),
        Record::new("track_data.track_layers._TrackLayer_instancecount_", 0, "1"),
        Record::new(&format!("{layer}._metadata_"), 2, "f1_26"),
        Record::new(&format!("{layer}.game_setup_modes"), 2, "f1_26"),
        Record::new(&format!("{layer}.is_invalid"), 2, "False"),
        Record::new(&format!("{layer}.seasons"), 2, "season_2026"),
        Record::new(&format!("{layer}.weekend_formats"), 2, "default"),
        Record::new(
            &format!("{layer}.worldEri"),
            2,
            "eaid://environment_package/tracks/[FILENAME]/wep/[FILENAME]_f1_26.wep",
        ),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    }
}

#[test]
fn a_track_record_places_the_files_only_and_the_build_writes_the_track_files() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackrecord-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = record_track_game(&root);
    let store = Store::new(root.join("data"));
    let install = |id: &str, code: &str, name: &str, ops: bool, record: Option<&str>| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        let operations = match ops {
            true => r#"{ "assets": ["ops.json"] }"#,
            false => "{}",
        };
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {operations} }}"#
            ),
        )
        .unwrap();
        if ops {
            std::fs::write(
                src.join("ops.json"),
                r#"{ "schema_version": 1, "ops": [{"op":"track.add","source":"track"}] }"#,
            )
            .unwrap();
            std::fs::write(
                src.join("track/manifest.json"),
                format!(
                    r#"{{ "track_code": "{code}", "track_name": "{name}", "erp": "{code}.erp",
                         "activation": {{ "copy_from_track": "melbourne", "copy_slot": "melbourne" }} }}"#
                ),
            )
            .unwrap();
        }
        if let Some(record) = record {
            std::fs::create_dir_all(src.join("registry")).unwrap();
            std::fs::write(src.join("registry/track.json"), record).unwrap();
        }
        let world = format!("eaid://environment_package/tracks/{code}/wep/{code}_common.wep");
        std::fs::write(
            src.join(format!("track/{code}.erp")),
            archive_of("WEP", &[&world], 0),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    install("mod.okayama", "okayama", "Okayama", true, None);
    install("mod.sochi", "sochi", "Sochi Autodrom", true, None);
    install(
        "mod.testring",
        "testring",
        "Test Ring",
        true,
        Some(
            r#"{ "track": { "testring": { "code": "testring", "base": "game/melbourne", "files": "track/",
                                            "provides": ["venue/sepang"] } } }"#,
        ),
    );
    install(
        "mod.ring2",
        "ring2",
        "Ring Two",
        false,
        Some(
            r#"{ "track": { "ring2": { "code": "ring2", "name": "Ring Two", "base": "game/melbourne",
                                         "files": "track/" } } }"#,
        ),
    );

    let built = build(&store, "default", &game).unwrap();
    for code in ["testring", "ring2"] {
        let line = format!("{code}: files only; its track record adds its rows in game");
        assert!(built.lines.contains(&line), "{:?}", built.lines);
    }
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text("tracks._Track_instancecount_").as_deref(), Some("3"));
    let labels: Vec<String> = (0..3)
        .filter_map(|i| out.get_text(&format!("tracks.Track_{i}._metadata_")))
        .collect();
    assert_eq!(labels, ["melbourne", "sochi", "okayama"]);
    let placed: Vec<&String> = built
        .plan
        .data
        .keys()
        .filter(|k| k.contains("invitational_image"))
        .collect();
    assert_eq!(placed.len(), 4, "{placed:?}");

    let older: crate::tracks::OlderTracks = crate::read_json(&crate::stdlib::reports::path(
        &store.root,
        crate::tracks::OLDER_FILE,
    ))
    .unwrap();
    assert_eq!(older.profile, "default");
    let entry = |code: &str| {
        let t = older.tracks.iter().find(|t| t.code == code).unwrap();
        (t.addon.as_str(), t.slot.as_deref(), t.stat_id, t.venue.as_deref())
    };
    assert_eq!(older.tracks.len(), 2, "{:?}", older.tracks);
    assert_eq!(
        entry("okayama"),
        ("mod.okayama", None, Some(10), Some("venue/okayama"))
    );
    assert_eq!(
        entry("sochi"),
        ("mod.sochi", Some("game/sochi"), Some(9), Some("venue/sochi"))
    );
    let facts = crate::activation::facts::read(&store.root).unwrap();
    assert_eq!((facts.profile.as_str(), facts.layers.len()), ("default", 1));
    assert_eq!(facts.layers[0].world, "f1_26");
    let mut codes: Vec<&String> = facts.tracks.keys().collect();
    codes.sort();
    assert_eq!(codes, ["okayama", "ring2", "sochi", "testring"]);
    let written = std::fs::metadata(crate::activation::facts::path(&store.root))
        .unwrap()
        .modified()
        .unwrap();
    let again = build(&store, "default", &game).unwrap();
    assert_eq!(again.buildhash, built.buildhash);
    let kept = std::fs::metadata(crate::activation::facts::path(&store.root))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        kept, written,
        "a reused build with the same inputs keeps the facts file"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_track_rows_stat_id_is_an_added_row_id_and_a_lent_one_is_the_one_the_build_gives() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-tracklent-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let slot = dir.join(slot_file(SLOTS[0]));
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, archive(&["eaid://ui_package/slot.uil"], 0)).unwrap();
    let res = "tracks.track_resource.TrackResource_0";
    let set = "track_data.game_setup.TrackDataSet_0";
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.statId", 0, "5"),
        Record::new("tracks.Track_0.track_resource", 2, "2019"),
        Record::new("tracks.track_resource._TrackResource_instancecount_", 0, "1"),
        Record::new(&format!("{res}._metadata_"), 2, "2019"),
        Record::new(&format!("{res}.iblAssetGroupsRoot"), 2, "probes/"),
        Record::new(&format!("{res}.iblKey"), 2, "key_2019.assetgroup"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{set}._metadata_"), 2, "f1"),
        Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"),
        Record::new(&format!("{set}.TrackData_0._metadata_"), 2, "melbourne"),
        Record::new(&format!("{set}.TrackData_0.track"), 2, "melbourne"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let src = root.join("src").join("mod.testring");
    std::fs::create_dir_all(src.join("track")).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "mod.testring", "name": "Test Ring", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "assets": ["ops.json"] } }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("ops.json"),
        r#"{ "schema_version": 1, "ops": [{"op":"track.add","source":"track"}] }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("track/manifest.json"),
        r#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
             "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" } }"#,
    )
    .unwrap();
    let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
    std::fs::write(src.join("track/testring.erp"), archive_of("WEP", &[world], 0)).unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("mod.testring", true))
        .unwrap();
    let stat_id = |b: &Built| {
        let db = Gvdb::parse(&std::fs::read(&b.plan.data[GVDB]).unwrap()).unwrap();
        db.get_text("tracks.Track_1.statId").unwrap()
    };
    let row = crate::row_ids::track_row("testring");

    let own = build(&store, "default", &game).unwrap();
    assert_eq!(stat_id(&own), "6");
    assert_eq!(own.row_ids["mod.testring"][&row], 6, "{:?}", own.row_ids);
    let outputs: BTreeMap<String, BTreeMap<String, fingerprint::Output>> =
        crate::read_json(&store.root.join(fingerprint::OUTPUTS_FILE)).unwrap();
    assert_eq!(outputs["default"]["mod.testring"].row_ids[&row], 6);
    let kept = std::fs::read(crate::row_ids::path(&store.root)).unwrap();

    let host = crate::row_ids::Overrides {
        rows: BTreeMap::from([("mod.testring".to_owned(), BTreeMap::from([(row.clone(), 40)]))]),
        ..Default::default()
    };
    crate::row_ids::lend(Some(host));
    let lent = build(&store, "default", &game);
    crate::row_ids::lend(None);
    let lent = lent.unwrap();
    assert_eq!(stat_id(&lent), "40");
    assert_eq!(lent.row_ids["mod.testring"][&row], 40);
    assert_eq!(
        std::fs::read(crate::row_ids::path(&store.root)).unwrap(),
        kept,
        "row-ids.json keeps the player's own id"
    );
    let back = build(&store, "default", &game).unwrap();
    assert_eq!(stat_id(&back), "6");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn skips_removals_of_the_records_a_track_adds() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackremove-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let slot = dir.join(slot_file(SLOTS[0]));
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, archive(&["eaid://ui_package/slot.uil"], 0)).unwrap();
    let res = "tracks.track_resource.TrackResource_0";
    let set = "track_data.game_setup.TrackDataSet_0";
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.statId", 0, "5"),
        Record::new("tracks.Track_0.track_resource", 2, "2019"),
        Record::new("tracks.track_resource._TrackResource_instancecount_", 0, "1"),
        Record::new(&format!("{res}._metadata_"), 2, "2019"),
        Record::new(&format!("{res}.iblAssetGroupsRoot"), 2, "probes/"),
        Record::new(&format!("{res}.iblKey"), 2, "key_2019.assetgroup"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{set}._metadata_"), 2, "f1"),
        Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"),
        Record::new(&format!("{set}.TrackData_0._metadata_"), 2, "melbourne"),
        Record::new(&format!("{set}.TrackData_0.track"), 2, "melbourne"),
        Record::new("stock.key_0", 0, "0"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, name: &str, ops: &str, track: bool| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
        )
        .unwrap();
        if track {
            std::fs::write(
                src.join("track/manifest.json"),
                r#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" } }"#,
            )
            .unwrap();
            let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
            std::fs::write(src.join("track/testring.erp"), archive_of("WEP", &[world], 0)).unwrap();
        }
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    let track_keys = [
        "tracks.Track_1._metadata_",
        "tracks.Track_1.statId",
        "tracks.track_resource.TrackResource_1.iblKey",
        &format!("{set}.TrackData_1.track"),
    ];
    let remove =
        |key: &str| format!(r#"{{"op":"db.remove-record","key":"{key}","allow_stock_removal":true}},"#);
    let mut ops: String = track_keys.iter().map(|k| remove(k)).collect();
    ops += &remove("stock.key_0");
    ops += r#"{"op":"db.put-record","key":"mod.key_0","type":0,"value":"3130"},"#;
    ops += r#"{"op":"db.remove-record","key":"mod.key_0"}"#;
    install("t.orlen", "Scuderia AlphaTauri ORLEN", &ops, false);
    install(
        "mod.testring",
        "Test Ring",
        r#"{"op":"track.add","source":"track"}"#,
        true,
    );

    let built = build(&store, "default", &game).unwrap();
    let cloned = "testring: testring: new track row cloned from melbourne".to_owned();
    assert!(built.lines.contains(&cloned), "{:?}", built.lines);
    let line = "Scuderia AlphaTauri ORLEN: 4 removal(s) of database records the tracks add skipped; \
                the game crashes when a record it reads is missing";
    assert_eq!(
        built
            .warnings
            .iter()
            .filter(|w| w.contains("removal(s)"))
            .collect::<Vec<_>>(),
        [line],
        "{:?}",
        built.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    for key in track_keys {
        assert!(out.get(key).is_some(), "{key}");
    }
    assert_eq!(out.get_text("tracks._Track_instancecount_").as_deref(), Some("2"));
    assert!(out.get("stock.key_0").is_none());
    assert!(out.get("mod.key_0").is_none());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_added_track_gets_the_stock_ai_data_and_difficulties_of_a_seasons_own_ai_folder() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-ownai-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let slot = dir.join(slot_file(SLOTS[0]));
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, archive(&["eaid://ui_package/slot.uil"], 0)).unwrap();
    let res = "tracks.track_resource.TrackResource_0";
    let set = "track_data.game_setup.TrackDataSet_0";
    let packages = "entrants.vehicles.vehicle_packages";
    let difficulty = |row: usize, folder: &str, field: &str| {
        format!("tracks.Track_{row}.ai_track_difficulties.{folder}.{field}")
    };
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.statId", 0, "5"),
        Record::new("tracks.Track_0.track_resource", 2, "2019"),
        Record::new(&difficulty(0, "f1_2025_vehicle_package", "time"), 8, "90.5,89.5"),
        Record::new(
            &difficulty(0, "f1_2025_vehicle_package", "max_slider_position"),
            0,
            "110",
        ),
        Record::new("tracks.track_resource._TrackResource_instancecount_", 0, "1"),
        Record::new(&format!("{res}._metadata_"), 2, "2019"),
        Record::new(&format!("{res}.iblAssetGroupsRoot"), 2, "probes/"),
        Record::new(&format!("{res}.iblKey"), 2, "key_2019.assetgroup"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{set}._metadata_"), 2, "f1"),
        Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"),
        Record::new(&format!("{set}.TrackData_0._metadata_"), 2, "melbourne"),
        Record::new(&format!("{set}.TrackData_0.track"), 2, "melbourne"),
        Record::new(&format!("{packages}._vehicle_package_instancecount_"), 0, "2"),
        Record::new(&format!("{packages}.vehicle_package_0._metadata_"), 2, "f1_2025"),
        Record::new(
            &format!("{packages}.vehicle_package_0.ai_package_folder"),
            2,
            "f1_2025_vehicle_package",
        ),
        Record::new(
            &format!("{packages}.vehicle_package_1._metadata_"),
            2,
            "f1cg_f1_2013",
        ),
        Record::new(
            &format!("{packages}.vehicle_package_1.ai_package_folder"),
            2,
            "f1cg_f1_2013_vehicle_package",
        ),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let ai = "eaid://environment_package/tracks/testring/ai";
    let install = |id: &str, ops: &str, track: bool| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{id}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
        )
        .unwrap();
        if track {
            std::fs::write(
                src.join("track/manifest.json"),
                r#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" } }"#,
            )
            .unwrap();
            let names = [
                "eaid://environment_package/tracks/testring/wep/testring_common.wep".to_owned(),
                format!("{ai}/f1_2025_vehicle_package/mclaren/mclaren.vp_times"),
                format!("{ai}/f1_2025_vehicle_package/lotus/lotus.vp_times"),
            ];
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            std::fs::write(src.join("track/testring.erp"), archive_of("WEP", &names, 0)).unwrap();
        }
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    let put = |key: &str, tag: u8, value: &str| {
        let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
        format!(r#"{{"op":"db.put-record","key":"{key}","type":{tag},"value":"{hex}"}}"#)
    };
    let own = "sw_test_vehicle_package";
    let ops = [
        put(&format!("{packages}.vehicle_package_1.ai_package_folder"), 2, own),
        put(&format!("ai.asset_data.{own}._asset_data_instancecount_"), 0, "2"),
        put(
            &format!("ai.asset_data.{own}.asset_data_0._metadata_"),
            2,
            "mclaren",
        ),
        put(
            &format!("ai.asset_data.{own}.asset_data_1._metadata_"),
            2,
            "cater",
        ),
        put(&difficulty(0, own, "time"), 8, "95.5,94.5"),
        put(&difficulty(0, own, "max_slider_position"), 0, "110"),
    ]
    .join(",");
    install("t.season", &ops, false);
    install("mod.testring", r#"{"op":"track.add","source":"track"}"#, true);

    let built = build(&store, "default", &game).unwrap();
    for line in [
        format!("testring: AI difficulty for {own} stands in with the track's f1_2025_vehicle_package"),
        format!(
            "testring: AI data for 2 teams of {own} stands in with the track's f1_2025_vehicle_package data of \
             the team of the same name; cater with lotus's"
        ),
    ] {
        assert!(built.lines.contains(&line), "{line}\n{:?}", built.lines);
    }
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(
        out.get_text("tracks.Track_1._metadata_").as_deref(),
        Some("testring")
    );
    assert_eq!(
        out.get_text(&difficulty(1, own, "time")).as_deref(),
        Some("90.5,89.5")
    );
    assert_eq!(
        out.get_text(&difficulty(1, own, "max_slider_position"))
            .as_deref(),
        Some("110")
    );
    assert_eq!(
        out.get_text(&difficulty(0, own, "time")).as_deref(),
        Some("95.5,94.5")
    );
    let placed: Vec<String> = built
        .plan
        .data
        .values()
        .chain(built.plan.added.values())
        .filter_map(|p| swofty_formats::erp::ErpFile::open(p).ok())
        .flat_map(|f| f.erp.resources.iter().map(|r| r.name.clone()).collect::<Vec<_>>())
        .filter(|n| n.starts_with(&format!("{ai}/{own}/")))
        .collect();
    assert_eq!(
        placed,
        [
            format!("{ai}/{own}/cater/cater.vp_times"),
            format!("{ai}/{own}/mclaren/mclaren.vp_times"),
        ]
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn keeps_the_track_changes_a_whole_database_mod_carries_back() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackkeep-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let slot = dir.join(slot_file(SLOTS[0]));
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, archive(&["eaid://ui_package/slot.uil"], 0)).unwrap();
    let res = "tracks.track_resource.TrackResource_0";
    let set = "track_data.game_setup.TrackDataSet_0";
    let list = "content_sets.content_set_0.available_tracks_0.tracks";
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.statId", 0, "5"),
        Record::new("tracks.Track_0.track_resource", 2, "2019"),
        Record::new("tracks.track_resource._TrackResource_instancecount_", 0, "1"),
        Record::new(&format!("{res}._metadata_"), 2, "2019"),
        Record::new(&format!("{res}.iblAssetGroupsRoot"), 2, "probes/"),
        Record::new(&format!("{res}.iblKey"), 2, "key_2019.assetgroup"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{set}._metadata_"), 2, "f1"),
        Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"),
        Record::new(&format!("{set}.TrackData_0._metadata_"), 2, "melbourne"),
        Record::new(&format!("{set}.TrackData_0.track"), 2, "melbourne"),
        Record::new("content_sets._content_set_instancecount_", 0, "1"),
        Record::new("content_sets.content_set_0._metadata_", 2, "2025"),
        Record::new(
            "content_sets.content_set_0._available_tracks_instancecount_",
            0,
            "1",
        ),
        Record::new(list, 13, "melbourne"),
        Record::new("stock.key_0", 0, "0"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, name: &str, ops: &str, track: bool| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
        )
        .unwrap();
        if track {
            std::fs::write(
                src.join("track/manifest.json"),
                r#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" } }"#,
            )
            .unwrap();
            let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
            std::fs::write(src.join("track/testring.erp"), archive_of("WEP", &[world], 0)).unwrap();
        }
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    install(
        "mod.testring",
        "Test Ring",
        r#"{"op":"track.add","source":"track"}"#,
        true,
    );
    let put = |key: &str, ty: u8, value: &str| {
        let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
        format!(r#"{{"op":"db.put-record","key":"{key}","type":{ty},"value":"{hex}"}}"#)
    };
    let ops = [
        put(list, 13, "melbourne"),
        put("tracks._Track_instancecount_", 0, "1"),
        put("stock.key_0", 0, "7"),
    ]
    .join(",");
    install("t.orlen", "Scuderia AlphaTauri ORLEN", &ops, false);

    let built = build(&store, "default", &game).unwrap();
    let line =
        "Scuderia AlphaTauri ORLEN: kept the tracks' changes to 2 database value(s) it sets back to the \
                game's own"
            .to_owned();
    assert!(built.lines.contains(&line), "{:?}", built.lines);
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text(list).as_deref(), Some("melbourne,testring"));
    assert_eq!(out.get_text("tracks._Track_instancecount_").as_deref(), Some("2"));
    assert_eq!(out.get_text("stock.key_0").as_deref(), Some("7"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_tracks_keep_their_menu_pictures_over_an_addon_that_ships_its_own() {
    use swofty_formats::erp::{Erp, ErpFile, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::gvdb::Record;
    use swofty_formats::pkg::{Arr, Node, Obj, Pkg, Val};
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-menus-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let write = |rel: &str, data: Vec<u8>| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    for slot in [SLOTS[0], SLOTS[1], crate::activation::ui::PICTURE_SLOT] {
        write(&slot_file(slot), archive(&["eaid://ui_package/slot.uil"], 0));
    }
    let res = "tracks.track_resource.TrackResource_0";
    let set = "track_data.game_setup.TrackDataSet_0";
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.statId", 0, "5"),
        Record::new("tracks.Track_0.track_resource", 2, "2019"),
        Record::new("tracks.track_resource._TrackResource_instancecount_", 0, "1"),
        Record::new(&format!("{res}._metadata_"), 2, "2019"),
        Record::new(&format!("{res}.iblAssetGroupsRoot"), 2, "probes/"),
        Record::new(&format!("{res}.iblKey"), 2, "key_2019.assetgroup"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{set}._metadata_"), 2, "f1"),
        Record::new(&format!("{set}._TrackData_instancecount_"), 0, "1"),
        Record::new(&format!("{set}.TrackData_0._metadata_"), 2, "melbourne"),
        Record::new(&format!("{set}.TrackData_0.track"), 2, "melbourne"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();

    let resource = |name: &str, kind: &str, fragments: Vec<Vec<u8>>| {
        let mut r = Resource {
            name: name.into(),
            kind: swofty_formats::erp::kind(kind).unwrap(),
            unk: vec![0; 4],
            fragments: fragments
                .into_iter()
                .map(|data| Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data))
                .collect(),
            hash: [0; 16],
        };
        r.hash = r.content_hash().unwrap();
        r
    };
    let surface = |name: &str, slices: &[u8]| {
        let mut head = vec![0u8; 40];
        head[12..16].copy_from_slice(&2u32.to_le_bytes());
        head[16..20].copy_from_slice(&2u32.to_le_bytes());
        head[28..32].copy_from_slice(&(slices.len() as u32).to_le_bytes());
        let data = slices.iter().flat_map(|b| [*b; 4]).collect();
        resource(&format!("{name}.image"), "GfxSurfaceRes", vec![head, data])
    };
    let view = |name: &str, slices: u32| {
        let mut v = vec![0u8; 0x18];
        v[0x14..].copy_from_slice(&slices.to_le_bytes());
        resource(name, "GfxSurfaceRes", vec![v])
    };
    let erp_of = |resources: Vec<Resource>| {
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources,
        }
        .to_bytes()
        .unwrap()
    };
    let obj = |pairs: Vec<(&str, Node)>| {
        Node::Obj(Obj::new(
            pairs.into_iter().map(|(k, v)| (Val::str(k), v)).collect(),
        ))
    };
    let list = |names: &[&str]| {
        let items = names
            .iter()
            .map(|n| {
                obj(vec![
                    ("Name", Node::Val(Val::str(*n))),
                    ("Value", Node::Val(Val::str("ui_package/stock.tif"))),
                ])
            })
            .collect();
        obj(vec![("Contents", Node::Arr(Arr::new(items)))])
    };
    let in_contents = |key: &str, inner: Node| obj(vec![(key, obj(vec![("Contents", inner)]))]);
    let pkg = |name: &str, root: Node| {
        let data = Pkg {
            name: *b"UIDB",
            root,
            vca: Vec::new(),
            layout: None,
        }
        .to_bytes()
        .unwrap();
        resource(name, "UIDB", vec![data])
    };
    const OVERRIDES: &str = "eaid://ui_package/databases/overrides.uidb";
    const MINIMAP: &str = "eaid://ui_package/databases/minimap.uidb";
    const UIDTA: &str = "eaid://ui_package/art/texture_arrays/ui_dynamic_texture_loading.uidta";
    const FLAGS: &str =
        "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.tracks.flags_circle.texarray";
    const TRACKS: &str =
        "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.fe_world.tracks.texarray";
    const LOGO: &str = "eaid://ui_package/art/textures/team_logo.tif";
    let stock = ["bahrain", "blank", "melbourne"];
    let overrides = obj(vec![(
        "overrides",
        obj(vec![(
            "Contents",
            obj(vec![
                (
                    "tracks",
                    obj(vec![("Contents", obj(vec![("flags_circle", list(&stock))]))]),
                ),
                (
                    "fe_world",
                    obj(vec![("Contents", obj(vec![("tracks", list(&stock))]))]),
                ),
            ]),
        )]),
    )]);
    let entry = obj(vec![("Name", Node::Val(Val::str("bahrain")))]);
    let minimap = in_contents(
        "hud",
        in_contents("minimap", in_contents("tracks", Node::Arr(Arr::new(vec![entry])))),
    );
    let ui = |rel: &str| format!("2025_asset_groups/ui_package/{rel}");
    let (front, hub) = (ui("common_front_end.erp"), ui("flow_career_hub.erp"));
    for rel in crate::activation::ui::UI_ERPS {
        let resources = match *rel {
            "databases.erp" => vec![pkg(OVERRIDES, overrides.clone()), pkg(MINIMAP, minimap.clone())],
            "flow_race.erp" => vec![pkg(MINIMAP, minimap.clone())],
            "persistent.erp" => vec![pkg(
                UIDTA,
                obj(vec![("TextureArrays", Node::Arr(Arr::new(Vec::new())))]),
            )],
            "common_front_end.erp" => vec![
                view(FLAGS, 3),
                surface(FLAGS, &[0, 1, 2]),
                view(TRACKS, 3),
                surface(TRACKS, &[0, 1, 2]),
                surface(LOGO, &[0]),
            ],
            "flow_career_hub.erp" => vec![view(FLAGS, 3), surface(FLAGS, &[0, 1, 2])],
            _ => vec![view("eaid://ui_package/other.tif", 1)],
        };
        write(&ui(rel), erp_of(resources));
    }
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, ops: &str, files: Vec<(&str, Vec<u8>)>| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{id}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
        )
        .unwrap();
        for (rel, data) in files {
            std::fs::write(src.join(rel), data).unwrap();
        }
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    for (code, pixel) in [("ring_a", 0xa0u8), ("ring_b", 0xb0)] {
        let picture = |key: &str| format!("eaid://ui_package/art/textures/track_installer/{code}/{key}.tif");
        let images = serde_json::json!({ "resources": {
            "flag_circle": picture("flag_circle"),
            "preview_small": picture("preview_small"),
        } });
        let manifest = serde_json::json!({
            "track_code": code,
            "track_name": code,
            "erp": format!("{code}.erp"),
            "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" },
            "ui": { "images_file": "images.json" },
        });
        let world = format!("eaid://environment_package/tracks/{code}/wep/{code}_common.wep");
        let archive = erp_of(vec![
            resource(&world, "WEP", vec![vec![0]]),
            view(&picture("flag_circle"), 1),
            surface(&picture("flag_circle"), &[pixel]),
            view(&picture("preview_small"), 1),
            surface(&picture("preview_small"), &[pixel + 1]),
        ]);
        install(
            &format!("mod.{code}"),
            r#"{"op":"track.add","source":"track"}"#,
            vec![
                ("track/manifest.json", manifest.to_string().into_bytes()),
                ("track/images.json", images.to_string().into_bytes()),
                (&format!("track/{code}.erp"), archive),
            ],
        );
    }
    install(
        "t.pack",
        &format!(
            r#"{{"op":"erp.overlay","erps":["{front}"],"source":"front.erp"}},
               {{"op":"erp.overlay","erps":["{hub}"],"source":"hub.erp"}}"#
        ),
        vec![
            (
                "front.erp",
                erp_of(vec![
                    view(FLAGS, 4),
                    surface(FLAGS, &[0, 1, 2, 9]),
                    view(TRACKS, 2),
                    surface(TRACKS, &[0, 9]),
                    surface(LOGO, &[7]),
                ]),
            ),
            (
                "hub.erp",
                erp_of(vec![view(FLAGS, 4), surface(FLAGS, &[0, 1, 2, 9])]),
            ),
        ],
    );

    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.warnings.contains(&kept_menus_line("t.pack")),
        "{:#?}",
        built.warnings
    );
    let slices = |rel: &str, name: &str| {
        let file = ErpFile::open(&built.plan.data[rel]).unwrap();
        let r = file.load(file.erp.find(name).unwrap()).unwrap();
        let data = r.fragments.last().unwrap().data().unwrap();
        match name.ends_with(".image") {
            true => data.chunks(4).map(|c| c[0]).collect::<Vec<u8>>(),
            false => data[0x14..0x18].to_vec(),
        }
    };
    let image = |name: &str| format!("{name}.image");
    assert_eq!(slices(&front, &image(FLAGS)), [0, 1, 2, 0xa0, 0xb0]);
    assert_eq!(slices(&front, FLAGS), 5u32.to_le_bytes());
    assert_eq!(slices(&front, &image(TRACKS)), [0, 1, 2, 0xa1, 0xb1]);
    assert_eq!(slices(&front, TRACKS), 5u32.to_le_bytes());
    assert_eq!(slices(&hub, &image(FLAGS)), [0, 1, 2, 0xa0, 0xb0]);
    assert_eq!(slices(&front, &image(LOGO)), [7]);
    let db = ErpFile::open(&built.plan.data[&ui("databases.erp")]).unwrap();
    let res = db.load(db.erp.find(OVERRIDES).unwrap()).unwrap();
    let pkg = Pkg::parse(&res.fragments[0].data().unwrap()).unwrap();
    let names = |a: &str, b: &str| -> Vec<String> {
        let mut cur = &pkg.root;
        for p in ["overrides", "Contents", a, "Contents", b, "Contents"] {
            cur = cur.as_obj().unwrap().get(p).unwrap();
        }
        cur.as_arr()
            .unwrap()
            .items
            .iter()
            .map(|n| {
                let name = n.as_obj().unwrap().get("Name").unwrap();
                name.as_val().unwrap().as_str().unwrap().to_owned()
            })
            .collect()
    };
    let indexed = ["bahrain", "blank", "melbourne", "ring_a", "ring_b"];
    assert_eq!(names("tracks", "flags_circle"), indexed);
    assert_eq!(names("fe_world", "tracks"), indexed);
    let pair = built
        .report
        .pairs
        .iter()
        .find(|p| p.loser == "t.pack")
        .unwrap_or_else(|| panic!("{:#?}", built.report.pairs));
    assert_eq!((pair.winner.as_str(), pair.resources), ("mod.ring_b", 6));
    assert!(pair.items.contains(&format!("{front}:{}", image(FLAGS))));
    assert_eq!(
        crate::profile::conflicts::last(&store, "default").unwrap().pairs,
        built.report.pairs
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn leaves_out_a_track_package_without_a_base_track() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-left-out-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let weekend = "schedule.activities.activity_race_weekend_0.track";
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 2, "melbourne"),
        Record::new("tracks.Track_0.laps", 0, "58"),
        Record::new(
            "schedule.activities._activity_race_weekend_instancecount_",
            0,
            "1",
        ),
        Record::new(
            "schedule.activities.activity_race_weekend_0._metadata_",
            2,
            "rw_melbourne",
        ),
        Record::new(weekend, 2, "melbourne"),
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 2, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let addon = |id: &str, name: &str, ops: &str, track: Option<&str>| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
        )
        .unwrap();
        if let Some(manifest) = track {
            std::fs::create_dir_all(src.join("track")).unwrap();
            std::fs::write(src.join("track/manifest.json"), manifest).unwrap();
        }
        store.install_addon(&src).unwrap();
    };
    addon(
        "mod.portimao",
        "Portimao",
        r#"{"op":"track.add","source":"track"},
           {"op":"db.set","select":"tracks.Track[_metadata_=portimao]","field":"laps","value":"66"}"#,
        Some(r#"{ "track_code": "portimao", "track_name": "Portimao", "erp": "portimao.erp" }"#),
    );
    addon(
        "t.calendar",
        "Calendar",
        r#"{"op":"db.set","select":"schedule.activities.activity_race_weekend[_metadata_=rw_melbourne]","field":"track","value":"portimao"},
           {"op":"db.set","select":"entrants.teams.team[_metadata_=mclaren_26]","field":"stat_id","value":"7"}"#,
        None,
    );
    let left_out = "Portimao (mod.portimao): track Portimao is left out; its manifest.json doesn't say which track it replaces or copies. Import a fixed version, or ask its author for one";
    let restored =
        "race weekend rw_melbourne: races at melbourne again; track portimao is left out of this build";

    store
        .update_profile("default", |p| p.set_enabled("t.calendar", true))
        .unwrap();
    let without = build(&store, "default", &game).unwrap();
    assert!(
        !without.warnings.iter().any(|w| w.contains("left out")),
        "{:?}",
        without.warnings
    );
    let off = "Calendar sets race weekend rw_melbourne to portimao, which is off: turn Portimao on; until then \
               this build puts back melbourne";
    assert!(
        without.warnings.iter().any(|w| w == off),
        "{:?}",
        without.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&without.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text(weekend).as_deref(), Some("melbourne"));

    store
        .update_profile("default", |p| p.set_enabled("mod.portimao", true))
        .unwrap();
    for pass in ["built", "reused"] {
        let built = build(&store, "default", &game).unwrap();
        let reused = built.lines.iter().any(|l| l.ends_with("reusing it"));
        assert_eq!(reused, pass == "reused");
        let warnings = &built.warnings;
        assert!(warnings.iter().any(|w| w == left_out), "{pass}: {warnings:?}");
        assert!(warnings.iter().any(|w| w == restored), "{pass}: {warnings:?}");
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("Portimao: change skipped: ") && w.contains("portimao")),
            "{pass}: {warnings:?}"
        );
        let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
        assert_eq!(out.get_text(weekend).as_deref(), Some("melbourne"), "{pass}");
        assert_eq!(
            out.get_text("tracks.Track_0.laps").as_deref(),
            Some("58"),
            "{pass}"
        );
        assert_eq!(
            out.get_text("tracks._Track_instancecount_").as_deref(),
            Some("1"),
            "{pass}"
        );
        assert_eq!(
            out.get_text("entrants.teams.team_0.stat_id").as_deref(),
            Some("7"),
            "{pass}"
        );
        let plan: serde_json::Value = crate::read_json(&built.overlay.join(PLAN_FILE)).unwrap();
        assert_eq!(plan["tracks"], serde_json::json!([]), "{pass}");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn leaves_out_the_tracks_the_game_s_track_lists_have_no_room_for() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-track-room-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let stock: Vec<String> = (0..64)
        .map(|i| format!("stock_{i:02}"))
        .chain(["madrid".to_owned()])
        .collect();
    let mut records = vec![
        Record::new("tracks._Track_instancecount_", 0, stock.len().to_string()),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new("track_data.game_setup.TrackDataSet_0._metadata_", 2, "f1"),
        Record::new(
            "track_data.game_setup.TrackDataSet_0._TrackData_instancecount_",
            0,
            stock.len().to_string(),
        ),
    ];
    for (i, code) in stock.iter().enumerate() {
        records.push(Record::new(
            &format!("tracks.Track_{i}._metadata_"),
            2,
            code.as_str(),
        ));
        records.push(Record::new(
            &format!("track_data.game_setup.TrackDataSet_0.TrackData_{i}.track"),
            2,
            code.as_str(),
        ));
    }
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let codes: Vec<String> = (0..22).map(|i| format!("added_{i:02}")).collect();
    for code in &codes {
        let id = format!("mod.{code}");
        let src = root.join("src").join(&id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{code}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            r#"{ "schema_version": 1, "ops": [{"op":"track.add","source":"track"}] }"#,
        )
        .unwrap();
        std::fs::write(
            src.join("track/manifest.json"),
            format!(
                r#"{{ "track_code": "{code}", "track_name": "{code}", "erp": "{code}.erp",
                     "activation": {{ "copy_from_track": "stock_00" }} }}"#
            ),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(&id, true))
            .unwrap();
    }
    let built = build(&store, "default", &game).unwrap();
    let notice = built
        .warnings
        .iter()
        .find(|w| w.starts_with("Too many tracks: "))
        .unwrap_or_else(|| panic!("{:?}", built.warnings));
    assert!(
        notice.starts_with(
            "Too many tracks: 22 track(s) are left out of this build because the game has room for 64 tracks, \
             64 of its own and 0 added ones"
        ),
        "{notice}"
    );
    assert!(notice.contains(": added_00, added_01, "), "{notice}");
    for code in &codes {
        let line = format!(
            "{code} (mod.{code}): track {code} is left out; the game has room for 64 tracks, 64 of its own and \
             0 added ones, and the track addons before it in the load order fill them. Turn some track addons \
             off to make room for it"
        );
        assert!(built.warnings.contains(&line), "{line}: {:?}", built.warnings);
    }
    if let Some(db) = built.plan.data.get(GVDB) {
        let out = Gvdb::parse(&std::fs::read(db).unwrap()).unwrap();
        assert_eq!(
            out.get_text("tracks._Track_instancecount_").as_deref(),
            Some("65")
        );
    }

    let career = |id: &str, name: &str, track: &str| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(
                r#"{{ "schema_version": 1, "ops": [{{"op":"db.set",
                     "select":"track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[track={track}]",
                     "field":"track","value":"{track}"}}] }}"#
            ),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    career("t.career", "Career", "added_21");
    let built = build(&store, "default", &game).unwrap();
    let skipped = "Career: change skipped: track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[track=added_21]: \
                   no row matches in track_data.game_setup.TrackDataSet_0.TrackData; it changes track added_21, \
                   which is left out of this build because the game has room for 64 tracks. Turn some track \
                   addons off to make room for it";
    assert!(
        built.warnings.iter().any(|w| w == skipped),
        "{:?}",
        built.warnings
    );

    career("t.other", "Other", "nowhere");
    let e = build(&store, "default", &game).unwrap_err().to_string();
    assert!(
        e.contains("applying t.other: track_data.game_setup.TrackDataSet[_metadata_=f1].TrackData[track=nowhere]: no row matches"),
        "{e}"
    );
    std::fs::remove_dir_all(&root).ok();
}

// A game whose database lists `stock` tracks with TrackData, and madrid.
fn track_room_game(root: &Path, stock: usize) -> Game {
    use swofty_formats::gvdb::Record;
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let codes: Vec<String> = (0..stock)
        .map(|i| format!("stock_{i:02}"))
        .chain(["madrid".to_owned()])
        .collect();
    let mut records = vec![
        Record::new("tracks._Track_instancecount_", 0, codes.len().to_string()),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new("track_data.game_setup.TrackDataSet_0._metadata_", 2, "f1"),
        Record::new(
            "track_data.game_setup.TrackDataSet_0._TrackData_instancecount_",
            0,
            codes.len().to_string(),
        ),
    ];
    for (i, code) in codes.iter().enumerate() {
        records.push(Record::new(
            &format!("tracks.Track_{i}._metadata_"),
            2,
            code.as_str(),
        ));
        records.push(Record::new(
            &format!("track_data.game_setup.TrackDataSet_0.TrackData_{i}.track"),
            2,
            code.as_str(),
        ));
    }
    std::fs::write(
        dir.join(GVDB),
        Gvdb {
            records: records.into(),
        }
        .to_bytes(),
    )
    .unwrap();
    Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    }
}

// Installs and turns on a track addon `mod.<code>`; `launcher` goes into its manifest as is.
fn track_room_addon(store: &Store, root: &Path, code: &str, launcher: &str) {
    let id = format!("mod.{code}");
    let src = root.join("src").join(&id);
    std::fs::create_dir_all(src.join("track")).unwrap();
    std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{code}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "assets": ["ops.json"] }}{launcher} }}"#
            ),
        )
        .unwrap();
    std::fs::write(
        src.join("ops.json"),
        r#"{ "schema_version": 1, "ops": [{"op":"track.add","source":"track"}] }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("track/manifest.json"),
        format!(
            r#"{{ "track_code": "{code}", "track_name": "{code}", "erp": "{code}.erp",
                     "activation": {{ "copy_from_track": "stock_00" }} }}"#
        ),
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(&id, true))
        .unwrap();
}

#[test]
fn the_track_check_fits_34_tracks_like_the_build_without_building() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-track-check-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = track_room_game(&root, 64);
    let store = Store::new(root.join("data"));
    let codes: Vec<String> = (0..34).map(|i| format!("added_{i:02}")).collect();
    for code in &codes {
        track_room_addon(&store, &root, code, "");
    }
    let all = crate::stdlib::deps::read_all(&store, &store.addons().unwrap());
    let started = std::time::Instant::now();
    let check = track_limit(&store, "default", &game, &all).unwrap();
    let took = started.elapsed();
    assert_eq!(check.added, 34);
    assert!(check.fitted);
    let limit = check.limit.clone().expect("no room for added tracks");
    assert_eq!(limit.names, codes);
    assert_eq!(limit.stock, 64);
    assert!(
        !store.overlays_dir("default").exists(),
        "the check built the profile"
    );
    assert!(
        !crate::profile::conflicts::path(&store, "default").exists(),
        "the check wrote a build report"
    );
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");

    let built = build(&store, "default", &game).unwrap();
    assert_eq!(built.track_limit, check.limit);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_track_check_under_the_fill_mark_reads_no_database() {
    let root = std::env::temp_dir().join(format!("swofty-build-track-check-few-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = track_room_game(&root, 43);
    let store = Store::new(root.join("data"));
    for i in 0..21 {
        track_room_addon(&store, &root, &format!("added_{i:02}"), "");
    }
    std::fs::remove_file(game.dir.join(GVDB)).unwrap();
    let all = crate::stdlib::deps::read_all(&store, &store.addons().unwrap());
    let check = track_limit(&store, "default", &game, &all).unwrap();
    assert_eq!(check.added, 21);
    assert!(!check.fitted);
    assert_eq!(check.limit, None);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn the_track_check_never_starts_an_addon_program_and_still_finds_the_limit() {
    let root = std::env::temp_dir().join(format!("swofty-build-track-check-helper-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = track_room_game(&root, 43);
    let store = Store::new(root.join("data"));
    let hung = r#", "launcher": { "program": { "linux": "bin/helper", "macos": "bin/helper" },
                       "sections": [{ "id": "items", "title": "Items", "icon": "package" }], "build": true }"#;
    let codes: Vec<String> = (0..28).map(|i| format!("added_{i:02}")).collect();
    for (i, code) in codes.iter().enumerate() {
        track_room_addon(&store, &root, code, if i == 3 { hung } else { "" });
    }
    let started_mark = root.join("helper-started");
    let program = store.addons_dir().join("mod.added_03").join("bin/helper");
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(
        &program,
        format!("#!/bin/sh\ntouch '{}'\nsleep 30\n", started_mark.display()),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(store.registry().unwrap().get("mod.added_03").unwrap().helper);
    let all = crate::stdlib::deps::read_all(&store, &store.addons().unwrap());
    let started = std::time::Instant::now();
    let check = track_limit(&store, "default", &game, &all).unwrap();
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
    assert!(!started_mark.exists(), "the check started the addon's program");
    assert_eq!(check.added, 28);
    let limit = check.limit.expect("7 tracks past the room for 21");
    assert_eq!(limit.names, codes[21..].to_vec());
    assert_eq!(limit.addons.len(), 7);
    std::fs::remove_dir_all(&root).ok();
}

fn record_addon_with_video(store: &Store, root: &Path, id: &str, code: &str, video: Option<(&str, &[u8])>) {
    let src = root.join("src").join(id);
    std::fs::create_dir_all(src.join("track")).unwrap();
    std::fs::create_dir_all(src.join("registry")).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        format!(
            r#"{{ "schema_version": 1, "id": "{id}", "name": "{code}", "version": "1.0.0", "author": "t",
                 "compat": {{ "game": "f1_25" }}, "operations": {{}} }}"#
        ),
    )
    .unwrap();
    let field = match video {
        Some((rel, bytes)) => {
            let at = src.join(rel);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, bytes).unwrap();
            format!(r#", "video": "{rel}""#)
        }
        None => String::new(),
    };
    std::fs::write(
        src.join("registry/track.json"),
        format!(
            r#"{{ "track": {{ "{code}": {{ "code": "{code}", "base": "game/melbourne", "files": "track/"{field} }} }} }}"#
        ),
    )
    .unwrap();
    let world = format!("eaid://environment_package/tracks/{code}/wep/{code}_common.wep");
    std::fs::write(
        src.join(format!("track/{code}.erp")),
        archive_of("WEP", &[&world], 0),
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(id, true))
        .unwrap();
}

#[test]
fn a_track_record_s_circuit_intro_is_placed_under_videos_and_named_in_the_track_facts() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackvideo-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = record_track_game(&root);
    let store = Store::new(root.join("data"));
    let mut bink = b"KB2j".to_vec();
    bink.extend([0u8; 64]);
    record_addon_with_video(
        &store,
        &root,
        "mod.vid",
        "vidring",
        Some(("videos/vidring.bk2", &bink)),
    );
    record_addon_with_video(&store, &root, "mod.plain", "plainring", None);
    record_addon_with_video(
        &store,
        &root,
        "mod.fake",
        "fakering",
        Some(("videos/fake.bk2", b"BIK1 not bink 2")),
    );
    record_addon_with_video(&store, &root, "mod.gone", "gonering", None);
    let gone = root.join("src/mod.gone/registry/track.json");
    std::fs::write(
        &gone,
        std::fs::read_to_string(&gone)
            .unwrap()
            .replace("\"files\"", "\"video\": \"videos/none.bk2\", \"files\""),
    )
    .unwrap();
    store.install_addon(&root.join("src/mod.gone")).unwrap();

    let built = build(&store, "default", &game).unwrap();
    let placed = "videos/track_map/swofty/mod.vid/vidring.bk2";
    let placed_26 = "videos/track_map/26/swofty/mod.vid/vidring.bk2";
    for at in [placed, placed_26] {
        assert_eq!(
            built.plan.added[at],
            store.addons_dir().join("mod.vid/videos/vidring.bk2")
        );
    }
    assert_eq!(
        built
            .plan
            .added
            .keys()
            .filter(|k| k.starts_with("videos/track_map/"))
            .count(),
        2,
        "{:?}",
        built.plan.added.keys().collect::<Vec<_>>()
    );
    for (label, at) in [
        ("circuit intro", placed),
        ("circuit intro of the F1 2026 box", placed_26),
    ] {
        assert!(
            built
                .lines
                .iter()
                .any(|l| l.starts_with(&format!("vidring: {label} "))
                    && l.ends_with(&format!("(video) goes in {at}"))),
            "{label}: {:?}",
            built.lines
        );
    }
    assert!(
        built.lines.contains(
            &"vidring: its TrackVideo row will hold track_map/swofty/mod.vid/vidring.bk2".to_owned()
        ),
        "{:?}",
        built.lines
    );
    for (id, why) in [
        ("mod.fake/fakering", "is not a Bink 2 file"),
        ("mod.gone/gonering", "is not in the addon"),
    ] {
        let line = built
            .warnings
            .iter()
            .find(|w| w.starts_with(&format!("track {id}: video ")))
            .unwrap_or_else(|| panic!("{:?}", built.warnings));
        assert!(
            line.contains(why) && line.ends_with("it plays its base track's intro"),
            "{line}"
        );
    }
    assert!(
        !built
            .warnings
            .iter()
            .any(|w| w.starts_with("track mod.plain/") || w.starts_with("track mod.vid/")),
        "{:?}",
        built.warnings
    );
    let facts = crate::activation::facts::read(&store.root).unwrap();
    let video = |code: &str| facts.tracks[code].video.clone();
    assert_eq!(
        video("vidring").as_deref(),
        Some("track_map/swofty/mod.vid/vidring.bk2")
    );
    for code in ["plainring", "fakering", "gonering"] {
        assert_eq!(video(code), None, "{code}");
    }
    let again = build(&store, "default", &game).unwrap();
    assert_eq!(again.buildhash, built.buildhash);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_track_record_s_per_box_videos_go_to_each_box_s_own_slot() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackvideos-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = record_track_game(&root);
    let store = Store::new(root.join("data"));
    let bink = |tag: u8| {
        let mut b = b"KB2j".to_vec();
        b.extend([tag; 64]);
        b
    };
    record_addon_with_video(&store, &root, "mod.many", "manyring", None);
    let src = root.join("src/mod.many");
    for (name, tag) in [("a", 1u8), ("b", 2), ("c", 3)] {
        std::fs::create_dir_all(src.join("videos")).unwrap();
        std::fs::write(src.join(format!("videos/{name}.bk2")), bink(tag)).unwrap();
    }
    let json = src.join("registry/track.json");
    std::fs::write(
        &json,
        std::fs::read_to_string(&json).unwrap().replace(
            "\"files\"",
            r#""video": { "f1": "videos/a.bk2", "f1_2026": "videos/b.bk2" }, "time_trial_video": "videos/c.bk2", "files""#,
        ),
    )
    .unwrap();
    store.install_addon(&src).unwrap();

    let built = build(&store, "default", &game).unwrap();
    let from = |tag: &str| store.addons_dir().join("mod.many/videos").join(tag);
    let expect = [
        ("videos/track_map/swofty/mod.many/manyring.bk2", "a.bk2"),
        ("videos/track_map/26/swofty/mod.many/manyring.bk2", "b.bk2"),
        ("videos/track_select/manyring.bk2", "c.bk2"),
        ("videos/track_select/26/manyring.bk2", "c.bk2"),
    ];
    for (at, file) in expect {
        assert_eq!(built.plan.added[at], from(file), "{at}");
    }
    let videos = |k: &&String| k.starts_with("videos/track_");
    assert_eq!(
        built.plan.added.keys().filter(videos).count(),
        4,
        "{:?}",
        built.plan.added.keys().collect::<Vec<_>>()
    );
    for (label, named_by) in [
        ("circuit intro", "video.f1"),
        ("circuit intro of the F1 2026 box", "video.f1_2026"),
        ("Time Trial background", "time_trial_video"),
        ("Time Trial background of the F1 2026 box", "time_trial_video"),
    ] {
        assert!(
            built
                .lines
                .iter()
                .any(|l| l.starts_with(&format!("manyring: {label} "))
                    && l.contains(&format!("({named_by}) goes in videos/"))),
            "{label}: {:?}",
            built.lines
        );
    }
    let facts = crate::activation::facts::read(&store.root).unwrap();
    assert_eq!(
        facts.tracks["manyring"].video.as_deref(),
        Some("track_map/swofty/mod.many/manyring.bk2")
    );
    let again = build(&store, "default", &game).unwrap();
    assert_eq!(again.buildhash, built.buildhash);
    std::fs::write(src.join("videos/c.bk2"), bink(9)).unwrap();
    store.install_addon(&src).unwrap();
    assert_ne!(
        build(&store, "default", &game).unwrap().buildhash,
        built.buildhash,
        "a changed Time Trial background is a new build"
    );

    std::fs::write(
        &json,
        std::fs::read_to_string(&json).unwrap().replace(
            r#""video": { "f1": "videos/a.bk2", "f1_2026": "videos/b.bk2" }, "#,
            "",
        ),
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    let only_wall = build(&store, "default", &game).unwrap();
    assert_eq!(only_wall.plan.added.keys().filter(videos).count(), 2);
    let facts = crate::activation::facts::read(&store.root).unwrap();
    assert_eq!(
        facts.tracks["manyring"].video, None,
        "a record with no circuit intro keeps its base's row"
    );
    std::fs::remove_dir_all(&root).ok();
}
