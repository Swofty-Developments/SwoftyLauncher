use super::*;

#[test]
fn a_row_id_a_conflict_check_gave_is_kept_by_the_build_after_it() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-check-ids-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let mut records = vec![Record::new("entrants.teams._team_instancecount_", 0, "2")];
    for (i, label) in ["mclaren_26", "gm_cadillac_26"].iter().enumerate() {
        let row = format!("entrants.teams.team_{i}");
        records.push(Record::new(&format!("{row}._metadata_"), 4, *label));
        records.push(Record::new(&format!("{row}.stat_id"), 0, (100 + i).to_string()));
    }
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.rows", "name": "Rows", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Rows(manifest)),
        locked: false,
        default_on: true,
    }]);
    let reused = |b: &Built| b.lines.iter().any(|l| l.starts_with("build: nothing changed"));
    let ids = crate::row_ids::path(&store.root);

    let checked = build_as(&store, "default", &game, true).unwrap();
    assert!(!ids.exists(), "a check keeps no ids");

    let built = build(&store, "default", &game).unwrap();
    assert_eq!(built.buildhash, checked.buildhash);
    assert!(!reused(&built), "{:?}", built.lines);
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(&ids).unwrap()).unwrap();
    assert_eq!(kept["rows"]["t.rows/entrants.teams.team/team12_26"]["id"], 102);
    assert!(reused(&build(&store, "default", &game).unwrap()));

    // With every id already kept, a check's build is as good as any.
    std::fs::remove_file(built.overlay.join(SAVED_FILE)).unwrap();
    build_as(&store, "default", &game, true).unwrap();
    assert!(reused(&build(&store, "default", &game).unwrap()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn build_reply_fields_name_the_columns_add_row_copies() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-fields-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let mut records = vec![Record::new("entrants.teams._team_instancecount_", 0, "2")];
    for (i, label) in ["mclaren_26", "gm_cadillac_26"].iter().enumerate() {
        let row = format!("entrants.teams.team_{i}");
        records.push(Record::new(&format!("{row}._metadata_"), 4, *label));
        records.push(Record::new(&format!("{row}.stat_id"), 0, (100 + i).to_string()));
    }
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.rows", "name": "Rows", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Rows(manifest)),
        locked: false,
        default_on: true,
    }]);
    let built = build(&store, "default", &game).unwrap();
    let db = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(db.instance_count("entrants.teams", "team"), Some(3));
    assert_eq!(
        db.get_text("entrants.teams.team_2._metadata_").as_deref(),
        Some("team12_26")
    );
    let key = "t.rows/entrants.teams.team/team12_26";
    let next_id = 102;
    assert_eq!(
        db.get_text("entrants.teams.team_2.stat_id"),
        Some(next_id.to_string())
    );
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.ends_with(&format!("stat_id {next_id} (the source's is 100)"))),
        "{:?}",
        built.lines
    );
    let outputs: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.root.join(fingerprint::OUTPUTS_FILE)).unwrap()).unwrap();
    assert_eq!(
        outputs["default"]["t.rows"]["row_ids"]["entrants.teams.team/team12_26"],
        next_id
    );
    let ids = crate::row_ids::path(&store.root);
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(&ids).unwrap()).unwrap();
    assert_eq!(kept["rows"][key]["id"], next_id);
    std::fs::write(
        &ids,
        format!(r#"{{"version":1,"rows":{{"{key}":{{"id":140,"from":"next"}}}}}}"#),
    )
    .unwrap();
    std::fs::remove_file(built.overlay.join(SAVED_FILE)).unwrap();
    let again = build(&store, "default", &game).unwrap();
    let db = Gvdb::parse(&std::fs::read(&again.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(
        db.get_text("entrants.teams.team_2.stat_id").as_deref(),
        Some("140")
    );
    assert!(
        again
            .lines
            .iter()
            .any(|l| l.ends_with("stat_id 140 (the source's is 100; kept from row-ids.json)")),
        "{:?}",
        again.lines
    );
    std::fs::remove_file(&ids).unwrap();
    std::fs::rename(&again.overlay, store.overlays_dir("default").join("older")).unwrap();
    let seeded = build(&store, "default", &game).unwrap();
    assert!(
        seeded
            .lines
            .iter()
            .any(|l| l.ends_with("stat_id 140 (the source's is 100; as the last build gave it)")),
        "{:?}",
        seeded.lines
    );
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(&ids).unwrap()).unwrap();
    assert_eq!(
        kept["rows"][key],
        serde_json::json!({ "id": 140, "from": "seeded" })
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_room_launch_build_uses_the_hosts_ids_under_its_own_cache_key_and_keeps_row_ids_json() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-lent-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let mut records = vec![Record::new("entrants.teams._team_instancecount_", 0, "2")];
    for (i, label) in ["mclaren_26", "gm_cadillac_26"].iter().enumerate() {
        let row = format!("entrants.teams.team_{i}");
        records.push(Record::new(&format!("{row}._metadata_"), 4, *label));
        records.push(Record::new(&format!("{row}.stat_id"), 0, (100 + i).to_string()));
    }
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.lent", "name": "Lent", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Rows(manifest)),
        locked: false,
        default_on: true,
    }]);
    let own = build(&store, "default", &game).unwrap();
    let ids = crate::row_ids::path(&store.root);
    let kept = std::fs::read(&ids).unwrap();
    let stat_id = |b: &Built| {
        let db = Gvdb::parse(&std::fs::read(&b.plan.data[GVDB]).unwrap()).unwrap();
        db.get_text("entrants.teams.team_2.stat_id").unwrap()
    };
    assert_eq!(stat_id(&own), "102");
    let host = crate::row_ids::Overrides {
        rows: BTreeMap::from([(
            "t.lent".to_owned(),
            BTreeMap::from([("entrants.teams.team/team12_26".to_owned(), 140)]),
        )]),
        ..Default::default()
    };
    crate::row_ids::lend(Some(host));
    let lent = build(&store, "default", &game);
    crate::row_ids::lend(None);
    let lent = lent.unwrap();
    assert_ne!(
        lent.buildhash, own.buildhash,
        "the host's ids are part of the cache key"
    );
    assert_eq!(stat_id(&lent), "140");
    assert!(
        lent.lines
            .iter()
            .any(|l| l.contains("the room host's, for this session only")),
        "{:?}",
        lent.lines
    );
    assert_eq!(lent.row_ids["t.lent"]["entrants.teams.team/team12_26"], 140);
    assert_eq!(std::fs::read(&ids).unwrap(), kept, "row-ids.json is as it was");
    let back = build(&store, "default", &game).unwrap();
    assert_eq!(
        back.buildhash, own.buildhash,
        "the next normal build is the player's own"
    );
    assert_eq!(stat_id(&back), "102");
    let settings = crate::row_ids::Overrides {
        order: vec!["t.lent".to_owned()],
        options: BTreeMap::from([(
            "t.lent".to_owned(),
            BTreeMap::from([("laps".to_owned(), serde_json::json!(7))]),
        )]),
        ..Default::default()
    };
    crate::row_ids::lend(Some(settings));
    let ordered = build(&store, "default", &game);
    crate::row_ids::lend(None);
    let ordered = ordered.unwrap();
    assert!(
        ordered
            .lines
            .iter()
            .any(|l| l.contains("launch: using the host's load order and 1 option value(s) for 1 addon(s)")),
        "{:?}",
        ordered.lines
    );
    assert_ne!(
        ordered.buildhash, own.buildhash,
        "the host's settings are part of the cache key"
    );
    assert_eq!(
        store.profile("default").unwrap().options.get("t.lent"),
        None,
        "the profile keeps its own"
    );
    let registry = crate::row_ids::Overrides {
        registry: BTreeMap::from([(
            "t.lent".to_owned(),
            BTreeMap::from([("content.package:t.lent/season".to_owned(), 116)]),
        )]),
        ..Default::default()
    };
    crate::row_ids::lend(Some(registry));
    let registered = build(&store, "default", &game);
    crate::row_ids::lend(None);
    assert_ne!(
        registered.unwrap().buildhash,
        own.buildhash,
        "the host's registry ids are part of the cache key"
    );
    let clash = crate::row_ids::Overrides {
        rows: BTreeMap::from([(
            "t.lent".to_owned(),
            BTreeMap::from([("entrants.teams.team/team12_26".to_owned(), 101)]),
        )]),
        ..Default::default()
    };
    crate::row_ids::lend(Some(clash));
    let refused = build(&store, "default", &game);
    crate::row_ids::lend(None);
    let err = refused
        .map(|_| ())
        .expect_err("a stock id is refused")
        .to_string();
    assert!(err.contains("stat id 101"), "{err}");
    assert_eq!(std::fs::read(&ids).unwrap(), kept);
    std::fs::remove_dir_all(&root).ok();
}

struct Drivers(AddonManifest);

impl Addon for Drivers {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let reply: swofty_addon::helper::Built = serde_json::from_str(
            r#"{ "ops": [
                   { "op": "db.add-row", "table": "entrants.drivers",
                     "from": "entrants.drivers.driver[_metadata_=max_verstappen]",
                     "set": { "_metadata_": "dm_208", "stat_id": "208" } },
                   { "op": "db.add-row", "table": "entrants.entrant_lists.entrants",
                     "from": "entrants.entrant_lists.entrants.entrant[_metadata_=max_verstappen]",
                     "set": { "_metadata_": "dm_208", "driver": "dm_208" } }],
                 "fields": { "entrants.drivers.driver": ["_metadata_", "stat_id"],
                             "entrants.entrant_lists.entrants.entrant": ["_metadata_", "driver"] } }"#,
        )
        .expect("valid reply");
        let origin = Origin {
            addon: cx.cx.addon.into(),
            item: None,
        };
        Ok(AssetBatch {
            ops: reply
                .ops
                .into_iter()
                .map(|op| Op {
                    op,
                    origin: origin.clone(),
                })
                .collect(),
            fields: reply.fields,
            ..AssetBatch::default()
        })
    }
}

#[test]
fn warns_when_a_db_editor_addon_picks_a_driver_driver_manager_does_not_add() {
    use crate::dbedit::driver_manager::tests::{database, fixture};
    use crate::dbedit::package::Package;
    use crate::dbedit::{Catalog, module};
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-dm-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let cat = Catalog::from_gvdb(database(), String::new());
    std::fs::write(dir.join(GVDB), database().to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(&format!(
        r#"{{ "schema_version": 1, "id": "{}", "name": "Driver Manager", "version": "1.0.0",
             "author": "t", "compat": {{ "game": "f1_25" }} }}"#,
        driver_manager::ID
    ))
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Drivers(manifest)),
        locked: false,
        default_on: true,
    }]);
    fixture(&store.addon_data_dir(driver_manager::ID));
    let lineups = module("lineups").unwrap();
    let seat = lineups
        .fields
        .iter()
        .position(|f| f.id == "entrant_reference")
        .unwrap();
    let view = lineups.view(&cat, &Default::default(), None).unwrap();
    let mut seats = Package::create(&store, "Seats").unwrap();
    seats
        .edits
        .set(&cat, view.rows[0].cells[seat].key.unwrap(), "dm_208")
        .unwrap();
    let driver = swofty_formats::hash::fnv1a64("entrants.entrant_lists.entrants.entrant_0.driver");
    seats.edits.set(&cat, driver, "dm_209").unwrap();
    seats.save(&store, &cat).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(&seats.id, true))
        .unwrap();

    let tail = "the game would read a driver and entrant its database doesn't have";
    let picks = |built: &Built| -> Vec<String> {
        built
            .warnings
            .iter()
            .filter(|w| w.contains(" picks "))
            .cloned()
            .collect()
    };
    let built = build(&store, "default", &game).unwrap();
    let db = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(
        db.get_text("entrants.entrant_lists.entrant_list_0.entrant_0.entrant_reference")
            .as_deref(),
        Some("dm_208")
    );
    assert_eq!(
        picks(&built),
        [format!(
            "Seats: picks Ret Ired (dm_209), which Driver Manager doesn't add (its page marks the drivers it leaves out); {tail}"
        )]
    );

    store
        .update_profile("default", |p| p.set_enabled(driver_manager::ID, false))
        .unwrap();
    assert_eq!(
        picks(&build(&store, "default", &game).unwrap()),
        [format!(
            "Seats: picks Jacob Swofty (dm_208), Ret Ired (dm_209), Driver Manager drivers, and Driver Manager is off in this profile; {tail}"
        )]
    );
    std::fs::remove_dir_all(&root).ok();
}
