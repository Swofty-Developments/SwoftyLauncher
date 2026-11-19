use super::*;

struct Grid(AddonManifest);

impl Addon for Grid {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let op: AssetOp = serde_json::from_value(serde_json::json!({
            "op": "db.set", "select": "t.list[_metadata_=cs_f1_2016]", "field": "name", "value": "edited"
        }))
        .map_err(|e| swofty_addon::Error::Other(e.to_string()))?;
        Ok(AssetBatch {
            ops: vec![Op {
                op,
                origin: Origin {
                    addon: cx.cx.addon.into(),
                    item: Some("grid-editor".into()),
                },
            }],
            ..AssetBatch::default()
        })
    }
}

#[test]
fn an_editor_op_on_a_row_a_later_addon_adds_applies() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-late-row-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![
        Record::new("t._list_instancecount_", 0, "1"),
        Record::new("t.list_0._metadata_", 4, "f1"),
        Record::new("t.list_0.name", 4, "stock"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.grid", "name": "Grid", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Grid(manifest)),
        locked: false,
        default_on: true,
    }]);
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.seasons", "name": "Seasons", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "database": ["db.json"] } }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("db.json"),
        r#"{ "schema_version": 1, "fields": { "t.list": ["_metadata_", "name"] }, "ops": [
             { "op": "add-row", "table": "t", "from": "t.list[_metadata_=f1]", "set": { "_metadata_": "cs_f1_2016", "name": "seasons" } } ] }"#,
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.seasons", true))
        .unwrap();

    let built = build(&store, "default", &game).unwrap();
    assert_eq!(built.order, [CORE_ID, "t.grid", "t.seasons"]);
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text("t.list_1._metadata_").as_deref(), Some("cs_f1_2016"));
    assert_eq!(out.get_text("t.list_1.name").as_deref(), Some("edited"));

    store
        .update_profile("default", |p| p.set_enabled("t.seasons", false))
        .unwrap();
    let e = build(&store, "default", &game).unwrap_err().to_string();
    assert!(
        e.contains("applying t.grid (grid-editor): t.list[_metadata_=cs_f1_2016]: no row matches, even after every addon's changes"),
        "{e}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn applies_db_editor_packages_leniently_in_load_order() {
    use crate::dbedit::Catalog;
    use crate::dbedit::package::Package;
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-dbedit-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 4, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
        Record::new("entrants.teams.team_0.prestige", 0, "5"),
    ];
    let stock = Gvdb::new(records).to_bytes();
    std::fs::write(dir.join(GVDB), &stock).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let cat = Catalog::load(&stock).unwrap();
    let key = |p: &str| swofty_formats::hash::fnv1a64(p);
    let mut first = Package::create(&store, "First").unwrap();
    first
        .edits
        .set(&cat, key("entrants.teams.team_0.stat_id"), "7")
        .unwrap();
    first
        .edits
        .set(&cat, key("entrants.teams.team_0.prestige"), "6")
        .unwrap();
    first.save(&store, &cat).unwrap();
    let mut second = Package::create(&store, "Second").unwrap();
    second
        .edits
        .set(&cat, key("entrants.teams.team_0.prestige"), "9")
        .unwrap();
    second.save(&store, &cat).unwrap();
    store
        .update_profile("default", |p| {
            p.set_enabled(&first.id, true)?;
            p.set_enabled(&second.id, true)
        })
        .unwrap();

    let built = build(&store, "default", &game).unwrap();
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(
        out.get_text("entrants.teams.team_0.stat_id").as_deref(),
        Some("7")
    );
    assert_eq!(
        out.get_text("entrants.teams.team_0.prestige").as_deref(),
        Some("9")
    );
    assert!(build(&store, "vanilla", &game).unwrap().lines.is_empty());

    first.edits.edits[0].select = Some("entrants.teams.team[_metadata_=gone_26]".into());
    first
        .edits
        .set(&cat, key("entrants.teams.team_0._metadata_"), "mclaren_27")
        .unwrap();
    first.save(&store, &cat).unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built
            .warnings
            .iter()
            .any(|w| w.starts_with("First: change skipped: ") && w.contains("gone_26")),
        "{:?}",
        built.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(
        out.get_text("entrants.teams.team_0.stat_id").as_deref(),
        Some("100")
    );
    assert_eq!(
        out.get_text("entrants.teams.team_0._metadata_").as_deref(),
        Some("mclaren_27")
    );

    let put = |path: &str, value: &str| serde_json::json!({"op": "put-record", "key": format!("0x{:016x}", key(path)), "tag": 0, "value": value});
    let hidden = serde_json::json!({"schema_version": 1, "ops": [
        put("entrants.teams.team_0.prestige", "77"), put("entrants.teams.team_0.stat_id", "55")]});
    let addon = store.addons_dir().join(&second.id);
    std::fs::write(addon.join("database.json"), hidden.to_string()).unwrap();
    std::fs::write(
        store.addons_dir().join(&first.id).join("dbedit.json"),
        "{\"format\": \"swofty-db-pa",
    )
    .unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built
            .warnings
            .iter()
            .any(|w| w.starts_with("First: package skipped: ")),
        "{:?}",
        built.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    for (path, want) in [
        ("entrants.teams.team_0.prestige", "9"),
        ("entrants.teams.team_0.stat_id", "100"),
        ("entrants.teams.team_0._metadata_", "mclaren_26"),
    ] {
        assert_eq!(out.get_text(path).as_deref(), Some(want), "{path}");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_row_count_past_the_rows_there_stays_at_the_rows() {
    use crate::dbedit::Catalog;
    use crate::dbedit::package::Package;
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-short-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let list = "schedule.track_lists.track_event_list_0";
    let count = format!("{list}._fixed_event_reference_instancecount_");
    let mut records = vec![
        Record::new("schedule.track_lists._track_event_list_instancecount_", 0, "1"),
        Record::new(&format!("{list}._metadata_"), 2, "F1_2025_16"),
        Record::new(&count, 0, "2"),
    ];
    for (i, e) in ["rw_bahrain", "rw_suzuka"].iter().enumerate() {
        let row = format!("{list}.fixed_event_reference_{i}");
        records.push(Record::new(&format!("{row}._metadata_"), 2, format!("ref_{e}")));
        records.push(Record::new(&format!("{row}.event_name"), 4, *e));
    }
    let stock = Gvdb::new(records).to_bytes();
    std::fs::write(dir.join(GVDB), &stock).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let cat = Catalog::load(&stock).unwrap();
    let mut p = Package::create(&store, "2017 v2").unwrap();
    let key = |p: &str| swofty_formats::hash::fnv1a64(p);
    let second = format!("{list}.fixed_event_reference_1.event_name");
    p.edits.set(&cat, key(&count), "4").unwrap();
    p.edits.set(&cat, key(&second), "rw_bahrain").unwrap();
    p.save(&store, &cat).unwrap();
    store
        .update_profile("default", |pr| pr.set_enabled(&p.id, true))
        .unwrap();

    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.warnings.contains(&format!(
            "2017 v2: {list}.fixed_event_reference has 2 rows, not 4; the count stays at 2 (the game crashes on rows that don't exist)"
        )),
        "{:?}",
        built.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text(&count).as_deref(), Some("2"));
    assert_eq!(out.get_text(&second).as_deref(), Some("rw_bahrain"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_special_livery_with_an_unknown_team_kit_builds_with_an_empty_one() {
    use crate::dbedit::Catalog;
    use crate::dbedit::package::Package;
    use crate::game_db::livery_kits::{KIT_YEARS, TABLE};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-kits-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let year = format!("{KIT_YEARS}.team_customisation_for_year_0");
    let mut records = vec![
        Record::new(
            &format!("{KIT_YEARS}._team_customisation_for_year_instancecount_"),
            0,
            "1",
        ),
        Record::new(&format!("{year}._metadata_"), 2, "2026"),
        Record::new(&format!("{year}._team_customisation_instancecount_"), 0, "2"),
        Record::new(
            &format!("{year}.team_customisation_0._metadata_"),
            2,
            "2026-mclaren",
        ),
        Record::new(
            &format!("{year}.team_customisation_1._metadata_"),
            2,
            "2026-renault",
        ),
        Record::new(&format!("{TABLE}._se_livery_instancecount_"), 0, "2"),
    ];
    for (i, label) in ["mclaren_monaco_2026", "lotus_lx02"].iter().enumerate() {
        let row = format!("{TABLE}.se_livery_{i}");
        records.push(Record::new(&format!("{row}._metadata_"), 2, *label));
        records.push(Record::new(
            &format!("{row}.team_customisation"),
            4,
            "2026-mclaren",
        ));
    }
    let stock = Gvdb::new(records).to_bytes();
    std::fs::write(dir.join(GVDB), &stock).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));

    let built = build(&store, "default", &game).unwrap();
    assert!(
        !built.warnings.iter().any(|w| w.contains("team_customisation")),
        "{:?}",
        built.warnings
    );

    let cat = Catalog::load(&stock).unwrap();
    let mut p = Package::create(&store, "More Liveries").unwrap();
    let kit = format!("{TABLE}.se_livery_1.team_customisation");
    p.edits
        .set(&cat, swofty_formats::hash::fnv1a64(&kit), "2026-lotus")
        .unwrap();
    p.save(&store, &cat).unwrap();
    store
        .update_profile("default", |pr| pr.set_enabled(&p.id, true))
        .unwrap();

    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.warnings.contains(
            &"More Liveries: special livery lotus_lx02 has team_customisation 2026-lotus, and \
              entrants.team_customisation has no row with that label; the value is left empty (the game \
              crashes at start-up on a team kit it cannot find)"
                .to_owned()
        ),
        "{:?}",
        built.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text(&kit).as_deref(), Some(""));
    assert_eq!(
        out.get_text(&format!("{TABLE}.se_livery_0.team_customisation"))
            .as_deref(),
        Some("2026-mclaren")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn adds_a_driver_head_row_and_its_groups_for_a_head_archive_no_row_names() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-heads-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let rows = "entrants.animation.driver_heads.driver_head";
    let stock_heads = [
        ("lando_norris", "79", "male_driver", "False"),
        ("kimi_raikkonen", "45", "male_driver", "False"),
        ("male_ls222", "198", "male_ea", "True"),
        ("female_ls112", "214", "female_ea", "True"),
    ];
    let mut records = vec![Record::new(
        "entrants.animation.driver_heads._driver_head_instancecount_",
        0,
        stock_heads.len().to_string(),
    )];
    for (i, (label, id, folder, custom)) in stock_heads.iter().enumerate() {
        let female = folder.starts_with("female");
        let model =
            format!("eaid://[CHARACTER_PACKAGE]/heads/{folder}/{label}/idf/{label}_rig_head_morph_v1");
        let (model, female_rig) = match female {
            true => (String::new(), model),
            false => (model, String::new()),
        };
        let body = if female { "generic_female" } else { "generic_male" };
        for (field, tag, value) in [
            ("_metadata_", 2, *label),
            ("asset_group_path_format", 4, "driver_head_format"),
            (
                "asset_group_path_format_female_on_male_rig",
                4,
                "driver_head_format_female_male_rig",
            ),
            ("available_for_customisation", 5, *custom),
            ("body_type", 4, body),
            ("esports_name", 2, ""),
            ("face_morph_asset_group_path_format", 4, "face_morph_format"),
            (
                "face_morph_asset_group_path_format_female_on_male_rig",
                4,
                "face_morph_format_female_male_rig",
            ),
            ("has_morph_data", 5, "True"),
            ("model_path", 2, model.as_str()),
            ("model_path_female_rig", 2, female_rig.as_str()),
            ("stat_id", 0, *id),
        ] {
            records.push(Record::new(&format!("{rows}_{i}.{field}"), tag, value));
        }
    }
    records.extend([
        Record::new("entrants.drivers._driver_instancecount_", 0, "1"),
        Record::new("entrants.drivers.driver_0._metadata_", 2, "lando_norris"),
        Record::new("entrants.drivers.driver_0.head", 4, "lando_norris"),
    ]);
    let stock = Gvdb::new(records);
    std::fs::write(dir.join(GVDB), stock.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let heads = "2025_asset_groups/animation_package/scenes/cinematics/drivers/heads";
    let morphs = "2025_asset_groups/animation_package/scenes/cinematics/drivers/morph_poses";
    let model_in = |folder: &str, label: &str, rig: &str| {
        archive_of(
            "GfxModelRes",
            &[&format!(
                "eaid://character_package/heads/{folder}/{label}/idf/{label}_rig_head_morph_v1{rig}.idf?model"
            )],
            1,
        )
    };
    let model = |label: &str| model_in("male_driver", label, "");
    let install = |id: &str, name: &str, files: &[(&str, Vec<u8>)], extra: &str| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("files")).unwrap();
        let mut ops: Vec<String> = Vec::new();
        for (i, (target, bytes)) in files.iter().enumerate() {
            std::fs::write(src.join(format!("files/{i}.erp")), bytes).unwrap();
            ops.push(format!(
                r#"{{"op":"file.put","source":"files/{i}.erp","target":"{target}"}}"#
            ));
        }
        if !extra.is_empty() {
            ops.push(extra.to_owned());
        }
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
            format!(r#"{{ "schema_version": 1, "ops": [{}] }}"#, ops.join(",")),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    install(
        "mod.rio",
        "Rio facemod",
        &[
            (&format!("{heads}/rio_haryanto.erp"), model("rio_haryanto")),
            (
                &format!("{morphs}/rio_haryanto.erp"),
                archive_of("AnimSet", &["x"], 2),
            ),
            (&format!("{heads}/nomodel.erp"), archive(&["x"], 3)),
            (&format!("{heads}/lando_norris.erp"), model("lando_norris")),
        ],
        "",
    );
    install(
        "mod.amy",
        "Amy facemod",
        &[
            (&format!("{heads}/amy.erp"), model_in("female_driver", "amy", "")),
            (
                &format!("{heads}/amy_malerig.erp"),
                model_in("female_driver", "amy", "_malerig"),
            ),
            (&format!("{morphs}/amy.erp"), archive_of("AnimSet", &["x"], 4)),
            (
                &format!("{morphs}/amy_malerig.erp"),
                archive_of("AnimSet", &["x"], 5),
            ),
        ],
        "",
    );
    install(
        "t.own",
        "Own rows",
        &[(&format!("{heads}/own_head.erp"), model("own_head"))],
        &format!(
            r#"{{"op":"db.add-row","table":"entrants.animation.driver_heads","from":"{rows}[_metadata_=kimi_raikkonen]","set":{{"_metadata_":"own_head"}}}}"#
        ),
    );
    install(
        "t.pick",
        "Pick",
        &[],
        r#"{"op":"db.set","select":"entrants.drivers.driver[_metadata_=lando_norris]","field":"head","value":"rio_haryanto"}"#,
    );

    let built = build(&store, "default", &game).unwrap();
    let archive_path = format!("{heads}/rio_haryanto.erp");
    for line in [
        format!(
            "Rio facemod: adds driver head rio_haryanto (male, copied from male_ls222) for {archive_path} and registers 2 asset group(s)"
        ),
        format!(
            "Amy facemod: adds driver head amy (female, copied from female_ls112) for {heads}/amy.erp and registers 4 asset group(s)"
        ),
    ] {
        assert!(built.lines.contains(&line), "{line}: {:?}", built.lines);
    }
    assert!(
        built.warnings.contains(&format!(
            "Rio facemod: {heads}/nomodel.erp gets no driver head row: it holds no head model"
        )),
        "{:?}",
        built.warnings
    );
    assert!(
        !built.warnings.iter().any(|w| w.contains("side-data")),
        "{:?}",
        built.warnings
    );
    assert_eq!(
        built
            .lines
            .iter()
            .filter(|l| l.contains("adds driver head"))
            .count(),
        2
    );

    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    let n: usize = out
        .get_text("entrants.animation.driver_heads._driver_head_instancecount_")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(n, 7);
    let field_of = |label: &str, f: &str| {
        let row = (0..n)
            .find(|i| out.get_text(&format!("{rows}_{i}._metadata_")).as_deref() == Some(label))
            .unwrap_or_else(|| panic!("a {label} row"));
        out.get_text(&format!("{rows}_{row}.{f}")).unwrap_or_default()
    };
    let field = |f: &str| field_of("rio_haryanto", f);
    assert_eq!(
        field("model_path"),
        "eaid://[CHARACTER_PACKAGE]/heads/male_driver/rio_haryanto/idf/rio_haryanto_rig_head_morph_v1"
    );
    assert_eq!(field("model_path_female_rig"), "");
    assert_eq!(field("has_morph_data"), "True");
    assert_eq!(field("body_type"), "generic_male");
    assert_eq!(field("available_for_customisation"), "False");
    assert_eq!(
        field_of("amy", "model_path"),
        "eaid://[CHARACTER_PACKAGE]/heads/female_driver/amy/idf/amy_rig_head_morph_v1_malerig"
    );
    assert_eq!(
        field_of("amy", "model_path_female_rig"),
        "eaid://[CHARACTER_PACKAGE]/heads/female_driver/amy/idf/amy_rig_head_morph_v1"
    );
    assert_eq!(field_of("amy", "body_type"), "generic_female");
    let ids: BTreeSet<String> = (0..n)
        .filter_map(|i| out.get_text(&format!("{rows}_{i}.stat_id")))
        .collect();
    assert_eq!(ids.len(), n, "every head keeps its own id: {ids:?}");

    let groups = |addon: &str| -> Vec<(String, Option<String>, Option<String>)> {
        built
            .waiting
            .iter()
            .filter_map(|op| match &op.op {
                AssetOp::Group(GroupOp::Register {
                    group, erp, template, ..
                }) if op.origin.addon == addon => Some((group.clone(), erp.clone(), template.clone())),
                _ => None,
            })
            .collect()
    };
    let g = "asset_groups/animation_package/scenes/cinematics/drivers";
    assert_eq!(
        groups("mod.rio"),
        [
            (
                format!("{g}/heads/rio_haryanto.assetgroup"),
                Some(archive_path.clone()),
                Some(format!("{g}/heads/lando_norris.assetgroup"))
            ),
            (
                format!("{g}/morph_poses/rio_haryanto.assetgroup"),
                Some(format!("{morphs}/rio_haryanto.erp")),
                Some(format!("{g}/morph_poses/lando_norris.assetgroup"))
            ),
        ]
    );
    let amy: Vec<String> = groups("mod.amy")
        .into_iter()
        .map(|(group, _, template)| format!("{group} < {}", template.unwrap_or_default()))
        .collect();
    assert_eq!(
        amy,
        [
            format!("{g}/heads/amy.assetgroup < {g}/heads/female_ls493.assetgroup"),
            format!("{g}/heads/amy_malerig.assetgroup < {g}/heads/female_ls493_malerig.assetgroup"),
            format!("{g}/morph_poses/amy.assetgroup < {g}/morph_poses/female_ls493.assetgroup"),
            format!(
                "{g}/morph_poses/amy_malerig.assetgroup < {g}/morph_poses/female_ls493_malerig.assetgroup"
            ),
        ]
    );

    let listed = crate::game_db::heads::for_profile(&store, &game, "default")
        .unwrap()
        .heads;
    let labels: Vec<&str> = listed.iter().map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["amy", "lando_norris", "own_head", "rio_haryanto"]);
    let rio = listed.iter().find(|h| h.label == "rio_haryanto").unwrap();
    assert_eq!(rio.addon.as_deref(), Some("mod.rio"));
    assert_eq!(rio.addon_name.as_deref(), Some("Rio facemod"));
    assert!(!rio.female);
    assert!(listed.iter().find(|h| h.label == "amy").unwrap().female);
    assert_eq!(crate::game_db::heads::installed(&store).len(), 4);
    store
        .update_profile("default", |p| p.set_enabled("mod.rio", false))
        .unwrap();
    let listed = crate::game_db::heads::for_profile(&store, &game, "default")
        .unwrap()
        .heads;
    assert_eq!(
        listed.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(),
        ["amy", "own_head"]
    );

    let lando = "entrants.drivers.driver_0.head";
    assert_eq!(out.get_text(lando).as_deref(), Some("rio_haryanto"));
    assert!(!built.warnings.iter().any(|w| w.contains("wears head")));
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.warnings.contains(
            &"Pick: lando_norris wears head rio_haryanto, which no addon on in this profile adds, so it keeps the game's head lando_norris"
                .to_owned()
        ),
        "{:?}",
        built.warnings
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text(lando).as_deref(), Some("lando_norris"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn skips_removals_of_stock_records_so_runtime_edits_still_apply() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-stockremove-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let mut records = vec![
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 4, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
    ];
    records.extend((0..5).map(|i| Record::new(&format!("stock.key_{i}"), 0, i.to_string())));
    let stock = Gvdb::new(records);
    std::fs::write(dir.join(GVDB), stock.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, name: &str, ops: &str| {
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
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    };
    let put = |key: &str, value: &str| {
        format!(r#"{{"op":"db.put-record","key":"{key}","type":0,"value":"{value}"}},"#)
    };
    let remove = |key: &str| format!(r#"{{"op":"db.remove-record","key":"{key}"}}"#);
    let mut ops = put("entrants.teams.team_0.stat_id", "3135");
    for i in 0..3 {
        ops += &put(&format!("mod.key_{i}"), "3130");
    }
    for i in 0..5 {
        ops += &remove(&format!("stock.key_{i}"));
        ops += ",";
    }
    ops += &remove("entrants.teams.team_0.stat_id");
    ops += ",";
    ops += &remove("mod.key_0");
    install("t.peugeot", "Peugeot TotalEnergies F1 Team", &ops);

    let built = build(&store, "default", &game).unwrap();
    let line = "Peugeot TotalEnergies F1 Team: 6 removal(s) of stock database records skipped; \
                the game crashes when a record it reads is missing";
    assert_eq!(
        built
            .warnings
            .iter()
            .filter(|w| w.contains("stock database records"))
            .collect::<Vec<_>>(),
        [line],
        "{:?}",
        built.warnings
    );
    assert_eq!(
        built.report.stock_removals,
        BTreeMap::from([("t.peugeot".to_owned(), 6)])
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    for r in &stock.records {
        assert!(out.get_key(r.key).is_some(), "{:#018x}", r.key);
    }
    assert_eq!(
        out.get_text("entrants.teams.team_0.stat_id").as_deref(),
        Some("15")
    );
    assert!(out.get("mod.key_0").is_none());
    assert_eq!(out.get_text("mod.key_1").as_deref(), Some("10"));
    assert_eq!(db_edits(&stock, &out).map(|e| e.len()), Ok(3));

    install(
        "t.author",
        "Author",
        &format!(
            r#"{}, {{"op":"db.remove-record","key":"stock.key_4","allow_stock_removal":true}}"#,
            remove("mod.key_1")
        ),
    );
    let built = build(&store, "default", &game).unwrap();
    assert_eq!(
        built.report.stock_removals,
        BTreeMap::from([("t.peugeot".to_owned(), 6)])
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert!(out.get("mod.key_1").is_none());
    assert!(out.get("stock.key_4").is_none());
    assert!(out.get("stock.key_3").is_some());
    assert!(db_edits(&stock, &out).is_err());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn warns_of_the_entrant_lists_an_addon_renames_or_removes() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-keeplists-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let lists = "entrants.entrant_lists";
    let whitelist = "content_sets.content_set_0.available_entrant_lists_0.entrant_lists";
    let mut records = vec![
        Record::new(&format!("{lists}._entrant_list_instancecount_"), 0, "3"),
        Record::new("content_sets._content_set_instancecount_", 0, "1"),
        Record::new(
            "content_sets.content_set_0._available_entrant_lists_instancecount_",
            0,
            "1",
        ),
        Record::new(whitelist, 13, "f1_2026,f2_2026,f2_2025"),
    ];
    for (i, (name, id)) in [("f1_2026", "67"), ("f2_2026", "104"), ("f2_2025", "95")]
        .into_iter()
        .enumerate()
    {
        records.push(Record::new(
            &format!("{lists}.entrant_list_{i}._metadata_"),
            4,
            name,
        ));
        records.push(Record::new(&format!("{lists}.entrant_list_{i}.stat_id"), 0, id));
        records.push(Record::new(
            &format!("{lists}.entrant_list_{i}.display_name"),
            4,
            format!("lng_{name}"),
        ));
    }
    let stock = Gvdb::new(records);
    std::fs::write(dir.join(GVDB), stock.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let src = root.join("src").join("t.old");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.old", "name": "Old Team Pack", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "assets": ["ops.json"] } }"#,
    )
    .unwrap();
    let put = |key: &str, value: &str| {
        format!(
            r#"{{"op":"db.put-record","key":"{key}","type":0,"value":"{}"}}"#,
            hex(value.as_bytes())
        )
    };
    let ops = [
        put(&format!("{lists}._entrant_list_instancecount_"), "2"),
        put(&format!("{lists}.entrant_list_1._metadata_"), "f2_2025"),
        put(&format!("{lists}.entrant_list_1.stat_id"), "95"),
        put(whitelist, "f1_2026,f2_2025"),
    ]
    .join(",");
    std::fs::write(
        src.join("ops.json"),
        format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.old", true))
        .unwrap();

    let built = build(&store, "default", &game).unwrap();
    let line = "Old Team Pack: renames or removes the entrant list(s) f2_2026; F2 menus may crash with it";
    assert!(built.warnings.iter().any(|w| w == line), "{:?}", built.warnings);
    assert_eq!(
        built.report.kept_lists,
        BTreeMap::from([("t.old".to_owned(), vec!["f2_2026".to_owned()])])
    );
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(
        out.get_text(&format!("{lists}._entrant_list_instancecount_"))
            .as_deref(),
        Some("2")
    );
    assert_eq!(
        out.get_text(&format!("{lists}.entrant_list_1._metadata_"))
            .as_deref(),
        Some("f2_2025")
    );
    assert_eq!(out.get_text(&format!("{lists}.entrant_list_3._metadata_")), None);
    assert_eq!(out.get_text(whitelist).as_deref(), Some("f1_2026,f2_2025"));
    std::fs::remove_dir_all(&root).ok();
}
