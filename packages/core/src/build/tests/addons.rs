use super::*;

struct Hook(AddonManifest);

impl Addon for Hook {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let origin = Origin {
            addon: cx.cx.addon.into(),
            item: None,
        };
        Ok(AssetBatch {
            ops: vec![
                Op {
                    op: AssetOp::Warn {
                        message: format!("{} cars", cx.options["cars"]),
                    },
                    origin: origin.clone(),
                },
                Op {
                    op: AssetOp::Erp(crate::ops::ErpOp::PatchPkg {
                        erp: "2025_asset_groups/missing.erp".into(),
                        resource: "eaid://missing".into(),
                        fragment: 0,
                        edits: vec![crate::ops::PkgEdit::SetText {
                            node: "n".into(),
                            value: "v".into(),
                            localise: false,
                        }],
                    }),
                    origin,
                },
            ],
            lines: vec![
                format!("profile {}", cx.cx.profile.name),
                match cx
                    .cx
                    .addons()
                    .map(|a| (a.list(), a.set_enabled(cx.cx.addon, true)))
                {
                    Some((Ok(_), Ok(()))) => "turned on".to_owned(),
                    Some((listed, turned)) => format!("{:?} {:?}", listed.err(), turned.err()),
                    None => "no addons".to_owned(),
                },
            ],
            ..AssetBatch::default()
        })
    }
}

#[test]
fn ops_and_options_that_need_an_addon_apply_only_while_the_build_has_it_on() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-when-addon-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let fields = ["round", "sprint", "laps", "whole"];
    let records = std::iter::once(Record::new("t._list_instancecount_", 0, "1"))
        .chain(std::iter::once(Record::new("t.list_0._metadata_", 4, "f2")))
        .chain(
            fields
                .iter()
                .map(|f| Record::new(&format!("t.list_0.{f}"), 4, "off")),
        )
        .collect();
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let track = root.join("track");
    std::fs::create_dir_all(&track).unwrap();
    std::fs::write(
        track.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.istanbul", "name": "Istanbul Park", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    store.install_addon(&track).unwrap();
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.f2", "name": "F2", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" },
             "options": [
               { "id": "sprint", "type": "toggle", "default": true, "binds": "db" },
               { "id": "istanbul_park", "type": "toggle", "default": true, "binds": "db", "needs": ["t.istanbul"] }
             ],
             "operations": { "database": ["rounds.json", "whole.json"] } }"#,
    )
    .unwrap();
    let set = |field: &str, when: &str| {
        format!(
            r#"{{ "op": "set", "select": "t.list[_metadata_=f2]", "field": "{field}", "value": "on", "when": {when} }}"#
        )
    };
    std::fs::write(
        src.join("rounds.json"),
        format!(
            r#"{{ "schema_version": 1, "ops": [{}, {}, {}] }}"#,
            set("round", r#"{ "addon": "t.istanbul" }"#),
            set("sprint", r#"{ "addon": "t.istanbul", "sprint": true }"#),
            set("laps", r#"{ "istanbul_park": true }"#)
        ),
    )
    .unwrap();
    std::fs::write(
        src.join("whole.json"),
        r#"{ "schema_version": 1, "when": { "addon": "t.istanbul" }, "ops": [
             { "op": "set", "select": "t.list[_metadata_=f2]", "field": "whole", "value": "on" } ] }"#,
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.f2", true))
        .unwrap();
    let values = || {
        let built = build(&store, "default", &game).unwrap();
        let path = built
            .plan
            .data
            .get(GVDB)
            .cloned()
            .unwrap_or_else(|| game.dir.join(GVDB));
        let out = Gvdb::parse(&std::fs::read(path).unwrap()).unwrap();
        fields.map(|f| out.get_text(&format!("t.list_0.{f}")).unwrap())
    };
    assert_eq!(values(), ["off", "off", "off", "off"]);
    store
        .update_profile("default", |p| p.set_enabled("t.istanbul", true))
        .unwrap();
    assert_eq!(values(), ["on", "on", "on", "on"]);
    let f2 = store.registry().unwrap().manifests()["t.f2"].clone();
    store
        .update_profile("default", |p| p.set_option(&f2, "sprint", false.into()).map(drop))
        .unwrap();
    assert_eq!(values(), ["on", "off", "on", "on"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_option_whose_rounds_need_a_track_the_build_lacks_builds_as_off_and_says_so() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackless-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let tracks = ["melbourne", "spa_francorchamps", "portimao"];
    let mut records = vec![
        Record::new("tracks._Track_instancecount_", 0, "3"),
        Record::new(
            "schedule.activities._activity_race_weekend_instancecount_",
            0,
            "2",
        ),
        Record::new("schedule.all_track_events._fixed_event_instancecount_", 2, "2"),
        Record::new("t._list_instancecount_", 0, "1"),
        Record::new("t.list_0._metadata_", 4, "f2"),
    ];
    for (i, code) in tracks.iter().enumerate() {
        records.push(Record::new(&format!("tracks.Track_{i}._metadata_"), 4, *code));
    }
    for (i, code) in tracks[1..].iter().enumerate() {
        let weekend = format!("schedule.activities.activity_race_weekend_{i}");
        records.push(Record::new(&format!("{weekend}._metadata_"), 4, *code));
        records.push(Record::new(&format!("{weekend}.track"), 4, *code));
        let event = format!("schedule.all_track_events.fixed_event_{i}");
        records.push(Record::new(
            &format!("{event}._metadata_"),
            4,
            format!("rw_{code}"),
        ));
        records.push(Record::new(&format!("{event}.activity"), 4, *code));
    }
    let fields = ["istanbul", "spa", "portimao"];
    records.extend(
        fields
            .iter()
            .map(|f| Record::new(&format!("t.list_0.{f}"), 4, "off")),
    );
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.f2", "name": "F2", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" },
             "options": [
               { "id": "istanbul", "type": "toggle", "label": "Turkish GP", "default": true, "binds": "db" },
               { "id": "spa", "type": "toggle", "default": true, "binds": "db" },
               { "id": "portimao", "type": "toggle", "label": "Portuguese GP", "default": true, "binds": "db" }
             ],
             "operations": { "database": ["rounds.json"] } }"#,
    )
    .unwrap();
    let set = |field: &str| {
        format!(
            r#"{{ "op": "set", "select": "t.list[_metadata_=f2]", "field": "{field}", "value": "on", "when": {{ "{field}": true }} }}"#
        )
    };
    let row = |option: &str, from: &str| {
        format!(
            r#"{{ "op": "add-row", "table": "schedule.all_track_events",
                 "from": "schedule.all_track_events.fixed_event[_metadata_={from}]",
                 "set": {{ "_metadata_": "rw_{option}_27" }}, "when": {{ "{option}": true }} }}"#
        )
    };
    std::fs::write(
        src.join("rounds.json"),
        format!(
            r#"{{ "schema_version": 1, "ops": [
                 {{ "op": "add-row", "table": "schedule.activities",
                    "from": "schedule.activities.activity_race_weekend[_metadata_=spa_francorchamps]",
                    "set": {{ "_metadata_": "istanbul_park", "track": "istanbul_park" }}, "when": {{ "istanbul": true }} }},
                 {}, {}, {}, {}, {} ] }}"#,
            row("spa", "rw_spa_francorchamps"),
            row("portimao", "rw_portimao"),
            set("istanbul"),
            set("spa"),
            set("portimao")
        ),
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.f2", true))
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let path = built
        .plan
        .data
        .get(GVDB)
        .cloned()
        .unwrap_or_else(|| game.dir.join(GVDB));
    let out = Gvdb::parse(&std::fs::read(path).unwrap()).unwrap();
    let values = fields.map(|f| out.get_text(&format!("t.list_0.{f}")).unwrap());
    assert_eq!(values, ["off", "on", "off"]);
    assert_eq!(
        out.instance_count("schedule.activities", "activity_race_weekend"),
        Some(2)
    );
    let warned = |track: &str| {
        built
            .warnings
            .iter()
            .filter(|w| w.contains(&format!("adds rounds at {track},")))
            .count()
    };
    assert_eq!(
        (
            warned("istanbul_park"),
            warned("portimao"),
            warned("spa_francorchamps")
        ),
        (1, 1, 0),
        "{:#?}",
        built.warnings
    );
    assert!(
        built
            .warnings
            .iter()
            .any(|w| w.starts_with("F2: option Turkish GP adds rounds at istanbul_park"))
    );
    let f2 = store.registry().unwrap().manifests()["t.f2"].clone();
    assert_eq!(
        store.profile("default").unwrap().option(&f2, &f2.options[0]),
        serde_json::Value::Bool(true)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_track_check_and_the_career_warnings_share_one_read_of_the_game_database() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-shipped-once-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        Record::new("tracks.Track_0._metadata_", 4, "melbourne"),
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0.stat_id", 0, "7"),
        Record::new("t._list_instancecount_", 0, "1"),
        Record::new("t.list_0._metadata_", 4, "f2"),
        Record::new("t.list_0.spa", 4, "off"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.f2", "name": "F2", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" },
             "options": [ { "id": "spa", "type": "toggle", "default": true, "binds": "db" } ],
             "operations": { "database": ["rounds.json"] } }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("rounds.json"),
        r#"{ "schema_version": 1, "ops": [
             { "op": "set", "select": "t.list[_metadata_=f2]", "field": "spa", "value": "on", "when": { "spa": true } } ] }"#,
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.f2", true))
        .unwrap();
    let careers = crate::stdlib::careers::data_path(&store.root);
    std::fs::create_dir_all(careers.parent().unwrap()).unwrap();
    std::fs::write(&careers, r#"[{ "career": "c1", "label": "One", "team_raw": 5 }]"#).unwrap();
    let reads = || crate::build::originals::SHIPPED_READS.with(|n| n.get());
    let before = reads();
    let built = build(&store, "default", &game).unwrap();
    assert_eq!(reads() - before, 1, "{:#?}", built.warnings);
    let out = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(out.get_text("t.list_0.spa").as_deref(), Some("on"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn runs_the_build_hooks_of_builtin_addons() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-hook-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.hook", "name": "Hook", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" },
             "options": [{ "id": "cars", "type": "slider", "label": "Cars", "min": 22, "max": 30, "step": 2, "default": 22, "binds": "db" }] }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Hook(manifest)),
        locked: false,
        default_on: true,
    }]);
    let checked = build_as(&store, "default", &game, true).unwrap();
    assert!(
        checked.lines.contains(
            &"t.hook: None Some(Other(\"addons can't be changed while conflicts are checked\"))".to_owned()
        ),
        "{:?}",
        checked.lines
    );
    let built = build(&store, "default", &game).unwrap();
    assert_eq!(built.order, [CORE_ID, "t.hook"]);
    assert!(built.lines.contains(&"t.hook: profile default".to_owned()));
    assert!(built.lines.contains(&"t.hook: turned on".to_owned()));
    assert!(built.warnings.contains(&"t.hook: 22 cars".to_owned()));
    assert!(
        built
            .warnings
            .contains(&"t.hook: ERP operations skipped; set-text is not built yet".to_owned())
    );
    assert!(store.addon_data_dir("t.hook").is_dir());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn fingerprints_count_what_the_build_made_not_the_rest_of_the_folders() {
    if loader_dir().is_some() {
        return;
    }
    let hook = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.hook", "name": "Hook", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" },
             "options": [{ "id": "cars", "type": "slider", "label": "Cars", "min": 22, "max": 30, "step": 2, "default": 22, "binds": "db" }] }"#,
    )
    .unwrap();
    let prints = |name: &str, zip: &str, texture: &str| {
        let root = std::env::temp_dir().join(format!("swofty-build-print-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let dir = root.join("game");
        std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
        std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
        std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
        let game = Game {
            identity: Identity::detect(&dir, Some(1)).unwrap(),
            dir,
            user_reg: None,
        };
        let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
            addon: Arc::new(Hook(hook.clone())),
            locked: false,
            default_on: true,
        }]);
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "mod.track", "name": "Track", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }},
                     "operations": {{ "assets": ["ops.json"], "files": [{{ "source": "a.dds", "target": "x/a.dds" }}] }},
                     "origin": {{ "tool": "teamsimplified.mods", "tool_version": "0.11.6", "file": "{zip}", "sha256": "ab" }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(src.join("ops.json"), r#"{ "schema_version": 1, "ops": [] }"#).unwrap();
        std::fs::write(src.join("a.dds"), texture).unwrap();
        std::fs::write(
            src.join("import-report.txt"),
            format!("{zip} -> mod.track (Track)"),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled("mod.track", true))
            .unwrap();
        let history = store.addon_data_dir("t.hook");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(
            history.join("importer.json"),
            format!(r#"{{ "mods": {{ "{name}": {{ "made": {{ "id": "mod.{name}" }} }} }} }}"#),
        )
        .unwrap();
        let print = || {
            let p = store.profile("default").unwrap();
            fingerprint::fingerprint(&store, &p, &game.identity).unwrap()
        };
        let before = print();
        build(&store, "default", &game).unwrap();
        let after = print();
        std::fs::remove_dir_all(&root).ok();
        (before, after)
    };
    let (a, b) = (
        prints("a", "track.zip", "dds"),
        prints("b", "track(1).zip", "dds"),
    );
    assert_ne!(a.0.digest, b.0.digest);
    assert_eq!(a.1.digest, b.1.digest, "{:#?}\n{:#?}", a.1.addons, b.1.addons);
    let track = a.1.addons.iter().find(|x| x.id == "mod.track").unwrap();
    assert!(matches!(track.source, fingerprint::Source::Built(_)), "{track:?}");
    let detail = track.detail.as_ref().unwrap();
    assert!(
        detail.parts.contains_key("Assets") && detail.parts.contains_key("declared"),
        "{detail:?}"
    );
    assert_eq!(detail.paths, ["a.dds"]);
    let (_, other) = prints("c", "track.zip", "another texture");
    assert_ne!(other.digest, a.1.digest);
    let other_track = other.addons.iter().find(|x| x.id == "mod.track").unwrap();
    let other_detail = other_track.detail.as_ref().unwrap();
    assert_eq!(other_detail.paths, detail.paths);
    assert_ne!(other_detail.reads, detail.reads, "{other_detail:?}");
}

#[test]
fn an_enabled_addon_for_a_newer_launcher_is_left_out_with_a_warning() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-future-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let future = store.addons_dir().join("w3.hud");
    std::fs::create_dir_all(&future).unwrap();
    std::fs::write(
        future.join(crate::manifest::MANIFEST_FILE),
        r#"{ "schema_version": 1, "id": "w3.hud", "name": "Race HUD", "version": "0.1.0", "author": "t",
             "compat": { "game": "f1_25" }, "hud": { "widgets": [] } }"#,
    )
    .unwrap();
    store
        .update_profile("default", |p| p.set_enabled("w3.hud", true))
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(!built.order.iter().any(|id| id == "w3.hud"), "{:?}", built.order);
    assert!(
        built.warnings.contains(
            &"Race HUD is left out: it needs a newer Swofty Launcher (unknown field `widgets`)".to_owned()
        ),
        "{:?}",
        built.warnings
    );
    assert!(
        store
            .profile("default")
            .unwrap()
            .enabled()
            .any(|id| id == "w3.hud")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_built_addon_installs_and_builds() {
    use crate::builder::AddonBuilder;
    use crate::manifest::Category;
    use crate::ops::{DbOp, ErpOp};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-builder-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![Record::new("rules.laps", 0, "5")];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let (erp, uri) = ("2025_asset_groups/t.erp", "eaid://t/a.idf");
    std::fs::write(dir.join(erp), archive(&[uri], 0)).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.erp"), archive(&[uri], 9)).unwrap();
    std::fs::write(src.join("a.dds"), "dds").unwrap();
    let mut b = AddonBuilder::new("t.generated", "Generated", "t");
    b.category = Some(Category::Helmets);
    b.db(DbOp::PutRecord {
        key: "rules.laps".into(),
        tag: 0,
        value: "3132".into(),
    })
    .erp(ErpOp::PutResource {
        erp: erp.into(),
        resource: uri.into(),
        source: Source::addon("erp/a.erp"),
        rename: None,
    })
    .source("erp/a.erp", src.join("a.erp"))
    .text("*", "lng_laps", "Laps")
    .file(src.join("a.dds"), "x/a.dds");
    b.write(&root.join("out")).unwrap();
    let store = Store::new(root.join("data"));
    let m = store.install_addon(&root.join("out")).unwrap();
    assert_eq!(m.category, Some(Category::Helmets));
    store
        .update_profile("default", |p| p.set_enabled("t.generated", true))
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let db = Gvdb::parse(&std::fs::read(&built.plan.data[GVDB]).unwrap()).unwrap();
    assert_eq!(db.get_text("rules.laps").as_deref(), Some("12"));
    assert!(built.plan.data.contains_key(erp));
    assert_eq!(
        built.plan.added["x/a.dds"],
        store.addons_dir().join("t.generated/files/x/a.dds")
    );
    assert!(
        built
            .lines
            .contains(&"Generated: 1 database value, 1 archive resource, 1 file, 1 text string".to_owned()),
        "{:?}",
        built.lines
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn places_screens_under_the_loader_ui_dir() {
    let m = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "a.pages", "name": "S", "version": "1.0.0", "author": "a",
             "compat": { "game": "f1_25" },
             "operations": { "screens": [{ "name": "swofty_addon_0", "file": "ui/page0.ui" },
               { "name": "generated", "page": "options", "flow": "flow_options", "anchor": "settings_container" }] } }"#,
    )
    .unwrap();
    let files = screen_files(&[&m], Path::new("/addons")).unwrap();
    assert_eq!(
        files,
        BTreeMap::from([(
            "swofty/ui/swofty_addon_0.ui".to_owned(),
            PathBuf::from("/addons/a.pages/ui/page0.ui")
        )])
    );
    assert!(screen_files(&[&m, &m], Path::new("/addons")).is_err());
}

#[test]
fn an_op_file_is_read_with_its_option_templating_filled_from_the_profile() {
    let root = std::env::temp_dir().join(format!("swofty-read-op-file-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let m = AddonManifest::parse(
        r#"{
          "schema_version": 1, "id": "a.dnf", "name": "DNF", "version": "1.0.0", "author": "a",
          "compat": { "game": "f1_25" },
          "options": [
            { "id": "failures", "type": "toggle", "default": false, "binds": "db" },
            { "id": "chance", "type": "slider", "min": 0, "max": 10, "step": 1, "default": 2, "binds": "db" }
          ],
          "operations": { "database": ["db/dnf.json"] }
        }"#,
    )
    .unwrap();
    let path = root.join("dnf.json");
    std::fs::write(
        &path,
        r#"{"schema_version": 1, "ops": [
          {"op": "set", "select": "t.r", "field": "chance", "value": "{{chance}}"},
          {"op": "set", "select": "t.r", "field": "failures", "value": "1", "when": {"failures": true}}
        ]}"#,
    )
    .unwrap();
    let raw = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(OpFile::from_value(raw, FileKind::Database).is_err());
    let mut p = Profile::new("x");
    let file = read_op_file(&path, FileKind::Database, &m, &p, &OnAddons::default()).unwrap();
    assert_eq!(file.ops.len(), 1);
    p.set_option(&m, "failures", serde_json::json!(true)).unwrap();
    p.set_option(&m, "chance", serde_json::json!(7)).unwrap();
    let file = read_op_file(&path, FileKind::Database, &m, &p, &OnAddons::default()).unwrap();
    assert_eq!(file.ops.len(), 2);
    assert!(
        matches!(&file.ops[0], AssetOp::Db(crate::ops::DbOp::Set { value, .. }) if value.as_deref() == Some("7"))
    );
    std::fs::write(&path, r#"{"ops": [{"op": "set", "value": "{{missing}}"}]}"#).unwrap();
    assert!(read_op_file(&path, FileKind::Database, &m, &p, &OnAddons::default()).is_err());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_addon_op_with_a_placement_id_past_the_games_32_is_refused_by_name() {
    let file = |edits: &str| {
        OpFile::from_value(
            serde_json::from_str(&format!(
                r#"{{"schema_version":1,"ops":[
                    {{"op":"patch-pkg","erp":"a.erp","resource":"eaid://x.layer","edits":{edits}}},
                    {{"op":"remove-resource","erp":"a.erp","resource":"eaid://y"}}]}}"#
            ))
            .unwrap(),
            FileKind::Erp,
        )
        .unwrap()
    };
    let mut warnings = Vec::new();
    let mut ok = file(
        r#"[{"op":"clone-entry","at":"instances","src":"[name=v$GridSlot22]","as":"v$GridSlot23","set":{"overrides.VehPlacement.Id":23}}]"#,
    );
    crate::build::op_files::refuse_placements("t.grid", "erp.json", &mut ok.ops, &mut warnings);
    assert_eq!(ok.ops.len(), 2);
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut bad = file(
        r#"[{"op":"clone-entry","at":"instances","src":"[name=v$PitBox12]","as":"v$PitBox33","set":{"overrides.VehPlacement.Id":33}}]"#,
    );
    crate::build::op_files::refuse_placements("t.grid", "erp.json", &mut bad.ops, &mut warnings);
    assert_eq!(bad.ops.len(), 1, "only the refused patch-pkg is dropped");
    assert!(matches!(
        bad.ops[0],
        AssetOp::Erp(crate::ops::ErpOp::RemoveResource { .. })
    ));
    assert_eq!(
        warnings,
        [
            "t.grid: erp.json: refused a.erp eaid://x.layer: clone-entry v$PitBox33: overrides.VehPlacement.Id 33 is \
          outside 1..=32, the vehicle placements the game holds per kind"
        ]
    );
}

/// How a [`Timed`] hook ends.
#[derive(Clone, Copy)]
enum Ends {
    Answers,
    Fails(&'static str),
    Panics,
}

/// A build hook that takes `sleep` to answer and notes when it ran.
struct Timed {
    manifest: AddonManifest,
    sleep: std::time::Duration,
    ran: Arc<std::sync::Mutex<Vec<(String, std::time::Instant, std::time::Instant)>>>,
    ends: Ends,
}

impl Addon for Timed {
    fn manifest(&self) -> &AddonManifest {
        &self.manifest
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let start = std::time::Instant::now();
        std::thread::sleep(self.sleep);
        self.ran
            .lock()
            .unwrap()
            .push((cx.cx.addon.to_owned(), start, std::time::Instant::now()));
        match self.ends {
            Ends::Answers => Ok(AssetBatch {
                lines: vec![format!("{} answered", cx.cx.addon)],
                ..AssetBatch::default()
            }),
            Ends::Fails(why) => Err(swofty_addon::Error::Other(why.into())),
            Ends::Panics => panic!("{} gave up", cx.cx.addon),
        }
    }
}

type Ran = Arc<std::sync::Mutex<Vec<(String, std::time::Instant, std::time::Instant)>>>;

fn timed_store(name: &str, hooks: &[(&str, u64, Ends)]) -> (PathBuf, Store, Game, Ran) {
    let root = std::env::temp_dir().join(format!("swofty-build-hooks-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let ran: Ran = Arc::default();
    let builtins = hooks
        .iter()
        .map(|(id, ms, ends)| Builtin {
            addon: Arc::new(Timed {
                manifest: AddonManifest::parse(&format!(
                    r#"{{ "schema_version": 1, "id": "{id}", "name": "{id}", "version": "1.0.0", "author": "t",
                         "compat": {{ "game": "f1_25" }} }}"#
                ))
                .unwrap(),
                sleep: std::time::Duration::from_millis(*ms),
                ran: ran.clone(),
                ends: *ends,
            }),
            locked: false,
            default_on: true,
        })
        .collect();
    let store = Store::new(root.join("data")).with_builtins(builtins);
    (root, store, game, ran)
}

#[test]
fn build_hooks_run_side_by_side_and_their_replies_join_in_load_order() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game, ran) =
        timed_store("order", &[("t.a", 400, Ends::Answers), ("t.b", 0, Ends::Answers)]);
    let built = build(&store, "default", &game).unwrap();
    assert_eq!(built.order, [CORE_ID, "t.a", "t.b"]);
    let at = |line: &str| built.lines.iter().position(|l| l == line);
    let (a, b) = (at("t.a: t.a answered"), at("t.b: t.b answered"));
    assert!(a.is_some() && a < b, "{:?}", built.lines);
    let ran = ran.lock().unwrap().clone();
    let when = |id: &str| {
        ran.iter()
            .find(|(r, ..)| r == id)
            .map(|(_, start, end)| (*start, *end))
            .unwrap()
    };
    let ((_, a_end), (b_start, _)) = (when("t.a"), when("t.b"));
    assert!(b_start < a_end, "t.b's hook waited for t.a's to answer");
    std::fs::remove_dir_all(&root).ok();
    let (root, store, game, _) =
        timed_store("order", &[("t.a", 0, Ends::Answers), ("t.b", 400, Ends::Answers)]);
    let again = build(&store, "default", &game).unwrap();
    assert_eq!(
        again.buildhash, built.buildhash,
        "which hook answers first changes nothing"
    );
    assert_eq!(again.lines, built.lines);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_build_hook_that_fails_fails_the_build_with_its_error() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game, _) = timed_store(
        "fails",
        &[
            ("t.a", 0, Ends::Fails("the helper timed out")),
            ("t.b", 200, Ends::Answers),
        ],
    );
    let e = build(&store, "default", &game).unwrap_err();
    assert_eq!(e.to_string(), "building t.a: the helper timed out");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_build_hook_that_panics_panics_the_build() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game, _) =
        timed_store("panics", &[("t.a", 100, Ends::Panics), ("t.b", 0, Ends::Answers)]);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(&store, "default", &game)))
        .expect_err("the build panics");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()));
    assert_eq!(message.as_deref(), Some("t.a gave up"));
    std::fs::remove_dir_all(&root).ok();
}
