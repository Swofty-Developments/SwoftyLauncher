use super::*;

#[test]
fn put_targets_take_the_games_spelling_and_the_ers_pass_builds_on_the_addons_file() {
    use swofty_formats::bxml::{Document, Element, Kind};
    use swofty_formats::erp::{Erp, ErpFile, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-putcase-{}", std::process::id()));
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
    let el = |name: &str, attrs: &[(&str, &str)], children: Vec<Element>| Element {
        kind: Kind::Element,
        pad: 0,
        name: name.as_bytes().to_vec(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect(),
        children,
        end_pad: 0,
    };
    let uri = "eaid://f1_2026_vehicle_package/teams/mclaren/handling/mclaren.vtf";
    let handling = |deploy: &str| {
        let doc = Document {
            root: el(
                "VTF",
                &[],
                vec![el(
                    "EnergyRecoverySystem",
                    &[("numTrackSettingsTypes", "1")],
                    vec![el(
                        "TrackSettings",
                        &[("name", "0")],
                        vec![
                            el("Track", &[("value", "melbourne")], Vec::new()),
                            el("MediumDeployFactor", &[("value", deploy)], Vec::new()),
                            el("OverrideDetectionLapProgress", &[("value", "0.8")], Vec::new()),
                            el("OverrideActivationLapProgress", &[("value", "0.85")], Vec::new()),
                        ],
                    )],
                )],
            ),
            tail: Vec::new(),
        };
        let data = doc.to_bytes();
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: vec![Resource {
                name: uri.into(),
                kind: swofty_formats::erp::kind("VTF").unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data)],
                hash: [0; 16],
            }],
        }
        .to_bytes()
        .unwrap()
    };
    let team = "2025_asset_groups/f1_2026_vehicle_package/teams/mclaren/wep/mclaren.erp";
    std::fs::create_dir_all(dir.join(team).parent().unwrap()).unwrap();
    std::fs::write(dir.join(team), handling("0.8")).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, ops: &str, files: &[(&str, Vec<u8>)]| {
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
    let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
    install(
        "mod.testring",
        r#"{"op":"track.add","source":"track"}"#,
        &[
            (
                "track/manifest.json",
                br#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" },
                     "ers": { "OverrideDetectionLapProgress": 0.91, "OverrideActivationLapProgress": 0.95 } }"#
                    .to_vec(),
            ),
            ("track/testring.erp", archive_of("WEP", &[world], 0)),
        ],
    );
    let upper = "2025_asset_groups/F1_2026_vehicle_package/teams/mclaren/wep/mclaren.erp";
    install(
        "t.upper",
        &format!(r#"{{"op":"file.put","source":"u.erp","target":"{upper}"}}"#),
        &[("u.erp", handling("0.7"))],
    );
    let value = |archive: &Path, track: &str, key: &str| {
        let file = ErpFile::open(archive).unwrap();
        let r = file.erp.resources.iter().find(|r| r.name == uri).unwrap();
        let doc = Document::parse(&file.load(r).unwrap().fragments[0].data().unwrap()).unwrap();
        doc.root
            .child(b"EnergyRecoverySystem")
            .unwrap()
            .children
            .iter()
            .find(|c| c.child(b"Track").and_then(|t| t.attr(b"value")) == Some(track.as_bytes()))
            .and_then(|c| c.child(key.as_bytes()))
            .and_then(|f| f.attr(b"value"))
            .map(|v| String::from_utf8_lossy(v).into_owned())
    };
    let same = |plan: &Plan| {
        plan.data
            .keys()
            .chain(plan.added.keys())
            .filter(|k| k.eq_ignore_ascii_case(team))
            .cloned()
            .collect::<Vec<_>>()
    };

    let built = build(&store, "default", &game).unwrap();
    assert_eq!(same(&built.plan), [team.to_owned()]);
    let car = &built.plan.data[team];
    assert_eq!(
        value(car, "melbourne", "MediumDeployFactor").as_deref(),
        Some("0.7")
    );
    assert_eq!(
        value(car, "testring", "MediumDeployFactor").as_deref(),
        Some("0.7")
    );
    assert_eq!(
        value(car, "testring", "OverrideDetectionLapProgress").as_deref(),
        Some("0.91")
    );

    let other = "2025_asset_groups/f1_2026_vehicle_package/teams/mclaren/wep/MCLAREN.erp";
    install(
        "t.other",
        &format!(r#"{{"op":"file.put","source":"o.erp","target":"{other}"}}"#),
        &[("o.erp", handling("0.4"))],
    );
    install(
        "t.fresh",
        r#"{"op":"file.put","source":"f.bin","target":"Mixed_Case/New_File.bin"}"#,
        &[("f.bin", b"new".to_vec())],
    );
    let built = build(&store, "default", &game).unwrap();
    assert_eq!(same(&built.plan), [team.to_owned()]);
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.starts_with(&format!("files: {team} is put by both"))),
        "{:?}",
        built.lines
    );
    assert!(built.plan.added.contains_key("Mixed_Case/New_File.bin"));
    assert!(!built.plan.added.contains_key("mixed_case/new_file.bin"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_overlay_with_a_handling_file_never_loads_as_it_is() {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    let archive = |name: &str, kind: &str| {
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: vec![Resource {
                name: name.into(),
                kind: swofty_formats::erp::kind(kind).unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", 4, 0x10, STORED, b"data".to_vec())],
                hash: [0; 16],
            }],
        }
        .to_bytes()
        .unwrap()
    };
    let dir = std::env::temp_dir().join(format!("swofty-carries-handling-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let (vtf, other) = (dir.join("vtf.erp"), dir.join("other.erp"));
    std::fs::write(
        &vtf,
        archive(
            "eaid://f1_2026_vehicle_package/teams/ferrari/handling/ferrari.vtf",
            "VTF",
        ),
    )
    .unwrap();
    std::fs::write(
        &other,
        archive(
            "eaid://f1_2026_vehicle_package/teams/ferrari/art/a.tex",
            "Texture",
        ),
    )
    .unwrap();
    let ferrari = ["2025_asset_groups/f1_2026_vehicle_package/teams/ferrari/wep/ferrari.erp".to_owned()];
    let monza =
        ["2025_asset_groups/f1_2025_vehicle_package/teams/ferrari/wep/ferrari_monza_25.erp".to_owned()];
    let track = ["2025_asset_groups/track_package/a.erp".to_owned()];
    assert!(carries_handling(&vtf, &ferrari).unwrap());
    assert!(carries_handling(&vtf, &monza).unwrap());
    assert!(!carries_handling(&vtf, &track).unwrap());
    assert!(!carries_handling(&other, &ferrari).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn track_ers_settings_survive_an_addon_that_writes_the_handling_file() {
    use swofty_formats::bxml::{Document, Element, Kind};
    use swofty_formats::erp::{Erp, ErpFile, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-ers-{}", std::process::id()));
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
    let el = |name: &str, attrs: &[(&str, &str)], children: Vec<Element>| Element {
        kind: Kind::Element,
        pad: 0,
        name: name.as_bytes().to_vec(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect(),
        children,
        end_pad: 0,
    };
    let uri = "eaid://f1_2026_vehicle_package/teams/mclaren/handling/mclaren.vtf";
    let handling = |deploy: &str| {
        let doc = Document {
            root: el(
                "VTF",
                &[],
                vec![el(
                    "EnergyRecoverySystem",
                    &[("numTrackSettingsTypes", "1")],
                    vec![el(
                        "TrackSettings",
                        &[("name", "0")],
                        vec![
                            el("Track", &[("value", "melbourne")], Vec::new()),
                            el("MediumDeployFactor", &[("value", deploy)], Vec::new()),
                            el("OverrideDetectionLapProgress", &[("value", "0.8")], Vec::new()),
                            el("OverrideActivationLapProgress", &[("value", "0.85")], Vec::new()),
                        ],
                    )],
                )],
            ),
            tail: Vec::new(),
        };
        let data = doc.to_bytes();
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: vec![Resource {
                name: uri.into(),
                kind: swofty_formats::erp::kind("VTF").unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data)],
                hash: [0; 16],
            }],
        }
        .to_bytes()
        .unwrap()
    };
    let team = "2025_asset_groups/f1_2026_vehicle_package/teams/mclaren/wep/mclaren.erp";
    std::fs::create_dir_all(dir.join(team).parent().unwrap()).unwrap();
    std::fs::write(dir.join(team), handling("0.8")).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, ops: &str, files: &[(&str, Vec<u8>)]| {
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
    let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
    install(
        "mod.testring",
        r#"{"op":"track.add","source":"track"}"#,
        &[
            (
                "track/manifest.json",
                br#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" },
                     "ers": { "OverrideDetectionLapProgress": 0.91, "OverrideActivationLapProgress": 0.95 } }"#
                    .to_vec(),
            ),
            ("track/testring.erp", archive_of("WEP", &[world], 0)),
        ],
    );
    install(
        "t.handling",
        &format!(r#"{{"op":"erp.put-resource","erp":"{team}","resource":"{uri}","source":"h.erp"}}"#),
        &[("h.erp", handling("0.5"))],
    );
    let season = "2025_asset_groups/cs_2019_vehicle_package/teams/mclaren/wep/mclaren.erp";
    install(
        "t.season",
        &format!(r#"{{"op":"file.put","source":"cs.erp","target":"{season}"}}"#),
        &[("cs.erp", handling("0.6"))],
    );

    let built = build(&store, "default", &game).unwrap();
    for line in [
        "ers: 1 handling file(s) other addons change get the tracks' settings too",
        "ers: 1 handling file(s) in car archives addons put get the tracks' settings",
    ] {
        assert!(built.lines.contains(&line.to_owned()), "{:?}", built.lines);
    }
    let value = |archive: &Path, track: &str, key: &str| {
        let file = ErpFile::open(archive).unwrap();
        let r = file.erp.resources.iter().find(|r| r.name == uri).unwrap();
        let doc = Document::parse(&file.load(r).unwrap().fragments[0].data().unwrap()).unwrap();
        doc.root
            .child(b"EnergyRecoverySystem")
            .unwrap()
            .children
            .iter()
            .find(|c| c.child(b"Track").and_then(|t| t.attr(b"value")) == Some(track.as_bytes()))
            .and_then(|c| c.child(key.as_bytes()))
            .and_then(|f| f.attr(b"value"))
            .map(|v| String::from_utf8_lossy(v).into_owned())
    };
    let car = &built.plan.data[team];
    assert_eq!(
        value(car, "melbourne", "MediumDeployFactor").as_deref(),
        Some("0.5")
    );
    assert_eq!(
        value(car, "testring", "MediumDeployFactor").as_deref(),
        Some("0.5")
    );
    assert_eq!(
        value(car, "testring", "OverrideDetectionLapProgress").as_deref(),
        Some("0.91")
    );
    assert_eq!(
        value(car, "testring", "OverrideActivationLapProgress").as_deref(),
        Some("0.95")
    );
    let car = &built.plan.added[season];
    assert_eq!(
        value(car, "melbourne", "MediumDeployFactor").as_deref(),
        Some("0.6")
    );
    assert_eq!(
        value(car, "testring", "OverrideActivationLapProgress").as_deref(),
        Some("0.95")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_handling_record_on_a_file_nothing_ships_is_a_warning() {
    let map = |id: &str, target: handling::Target| HandlingMap {
        id: id.into(),
        target,
        plan: handling::Plan::default(),
        resources: None,
        base: None,
    };
    let addon = |path: &str| handling::Target::Addon {
        addon: "t.real".into(),
        path: path.into(),
    };
    let maps = vec![
        map("t.real/put", addon("cars/put.erp")),
        map("t.real/worn", addon("cars/redbull.erp")),
        map("t.real/stock", handling::Target::Stock("x/ferrari.erp".into())),
        map("t.real/lost", addon("cars/teams/mclaren/wep/mclaren.erp")),
    ];
    let liveries = vec![handling::Livery {
        id: "t.real/redbull".into(),
        addon: "t.real".into(),
        archive: "Cars/RedBull.erp".into(),
        maps: Vec::new(),
    }];
    let reached = BTreeSet::from(["t.real/put".to_owned()]);
    let warnings = unshipped(&maps, &reached, &liveries);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0]
            .starts_with("handling: t.real/lost: cars/teams/mclaren/wep/mclaren.erp in t.real isn't put"),
        "{warnings:?}"
    );
}

#[test]
fn a_handling_record_changes_the_stock_team_archive_and_an_addons_own() {
    use swofty_formats::bxml::{Document, Element, Kind};
    use swofty_formats::erp::{Erp, ErpFile, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-handling-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let slot = dir.join(slot_file(SLOTS[0]));
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, archive(&["eaid://ui_package/slot.uil"], 0)).unwrap();
    let records = vec![Record::new("tracks._Track_instancecount_", 0, "0")];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let el = |name: &str, attrs: &[(&str, &str)], children: Vec<Element>| Element {
        kind: Kind::Element,
        pad: 0,
        name: name.as_bytes().to_vec(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect(),
        children,
        end_pad: 0,
    };
    let leaf = |name: &str, v: &str| el(name, &[("value", v)], Vec::new());
    let uri = "eaid://f1_2026_vehicle_package/teams/mclaren/handling/mclaren.vtf";
    let handling = || {
        let doc = Document {
            root: el(
                "VTF",
                &[],
                vec![
                    el(
                        "GearBox",
                        &[],
                        vec![
                            leaf("GearRatio1st", "3.5"),
                            leaf("GearRatio2nd", "2.6"),
                            leaf("GearRatio3rd", "2.0"),
                            leaf("GearUpSpeedMultiple1st", "0.8"),
                            leaf("GearUpSpeedMultiple2nd", "0.9"),
                            leaf("GearUpSpeedMultiple3rd", "1.0"),
                        ],
                    ),
                    el("Chassis", &[], vec![leaf("Mass", "771")]),
                    el(
                        "EnergyRecoverySystem",
                        &[],
                        vec![el("Common", &[], vec![leaf("MaxDeployPower", "350000")])],
                    ),
                    el(
                        "Upgrade",
                        &[("name", "radUpgradeERS")],
                        vec![el("EnergyRecoverySystem", &[], vec![leaf("X", "1")])],
                    ),
                ],
            ),
            tail: Vec::new(),
        };
        let data = doc.to_bytes();
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: vec![Resource {
                name: uri.into(),
                kind: swofty_formats::erp::kind("VTF").unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data)],
                hash: [0; 16],
            }],
        }
        .to_bytes()
        .unwrap()
    };
    let team = "2025_asset_groups/f1_2026_vehicle_package/teams/mclaren/wep/mclaren.erp";
    std::fs::create_dir_all(dir.join(team).parent().unwrap()).unwrap();
    std::fs::write(dir.join(team), handling()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let season = "2025_asset_groups/f1_1990_vehicle_package/teams/mclaren/wep/mclaren_handling.erp";
    let src = root.join("src/t.cars");
    std::fs::create_dir_all(src.join("registry")).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.cars", "name": "t.cars", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "assets": ["ops.json"] },
             "options": [{ "id": "gears", "type": "choice", "values": ["six", "four"], "default": "six", "binds": "db" }] }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("ops.json"),
        format!(
            r#"{{ "schema_version": 1, "ops": [{{"op":"file.put","source":"h.erp","target":"{season}"}}] }}"#
        ),
    )
    .unwrap();
    std::fs::write(src.join("h.erp"), handling()).unwrap();
    std::fs::write(
        src.join("registry/handling.json"),
        format!(
            r#"{{ "vehicle.handling": {{
                "stock_mclaren": {{ "vtf": {{ "stock": "{team}" }}, "ers": null,
                    "edits": [{{ "path": "chassis.mass", "op": "scale", "value": 0.8 }}],
                    "gearbox": {{ "ratios": [3.0, 2.2] }} }},
                "own": {{ "vtf": "h.erp", "chassis": {{ "mass": 600 }} }},
                "$select": [{{ "ids": "t.cars/own", "when": {{ "option": "gears", "is": "four" }},
                    "patch": {{ "gearbox": {{ "ratios": [3.0, 2.0, 1.5, 1.0] }} }} }}]
            }} }}"#
        ),
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.cars", true))
        .unwrap();
    let read = |archive: &Path| {
        let file = ErpFile::open(archive).unwrap();
        let r = file.erp.resources.iter().find(|r| r.name == uri).unwrap();
        Document::parse(&file.load(r).unwrap().fragments[0].data().unwrap()).unwrap()
    };
    let value = |doc: &Document, section: &str, key: &str| {
        doc.root
            .child(section.as_bytes())
            .and_then(|s| s.child(key.as_bytes()))
            .and_then(|f| f.attr(b"value"))
            .map(|v| String::from_utf8_lossy(v).into_owned())
    };

    let built = build(&store, "default", &game).unwrap();
    let doc = read(&built.plan.data[team]);
    assert_eq!(value(&doc, "Chassis", "Mass").as_deref(), Some("616.8"));
    assert_eq!(value(&doc, "GearBox", "GearRatio1st").as_deref(), Some("3"));
    assert_eq!(value(&doc, "GearBox", "GearRatio3rd").as_deref(), Some("0"));
    assert!(
        doc.root
            .child(b"GearBox")
            .unwrap()
            .child(b"GearUpSpeedMultiple2nd")
            .is_none()
    );
    assert!(doc.root.child(b"EnergyRecoverySystem").is_none());
    let doc = read(&built.plan.added[season]);
    assert_eq!(value(&doc, "Chassis", "Mass").as_deref(), Some("600"));
    assert_eq!(value(&doc, "GearBox", "GearRatio3rd").as_deref(), Some("2.0"));
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.contains("handling t.cars/stock_mclaren -> mclaren.vtf")),
        "{:?}",
        built.lines
    );
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.contains("Chassis/Mass: 771 -> 616.8"))
    );

    store
        .update_profile("default", |p| {
            let m = store.addons().unwrap()["t.cars"].clone();
            p.set_option(&m, "gears", "four".into()).map(drop)
        })
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let doc = read(&built.plan.added[season]);
    assert_eq!(value(&doc, "GearBox", "GearRatio3rd").as_deref(), Some("1.5"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rebuild_links_the_handling_and_ers_edits_of_a_put_car_archive_from_the_cache() {
    use swofty_formats::bxml::{Document, Element, Kind};
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-put-edits-{}", std::process::id()));
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
    let el = |name: &str, attrs: &[(&str, &str)], children: Vec<Element>| Element {
        kind: Kind::Element,
        pad: 0,
        name: name.as_bytes().to_vec(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect(),
        children,
        end_pad: 0,
    };
    let leaf = |name: &str, v: &str| el(name, &[("value", v)], Vec::new());
    let uri = "eaid://f1_2026_vehicle_package/teams/mclaren/handling/mclaren.vtf";
    let handling = || {
        let doc = Document {
            root: el(
                "VTF",
                &[],
                vec![
                    el("Chassis", &[], vec![leaf("Mass", "771")]),
                    el(
                        "EnergyRecoverySystem",
                        &[("numTrackSettingsTypes", "1")],
                        vec![el(
                            "TrackSettings",
                            &[("name", "0")],
                            vec![
                                leaf("Track", "melbourne"),
                                leaf("MediumDeployFactor", "0.8"),
                                leaf("OverrideDetectionLapProgress", "0.8"),
                                leaf("OverrideActivationLapProgress", "0.85"),
                            ],
                        )],
                    ),
                ],
            ),
            tail: Vec::new(),
        };
        let data = doc.to_bytes();
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: vec![Resource {
                name: uri.into(),
                kind: swofty_formats::erp::kind("VTF").unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data)],
                hash: [0; 16],
            }],
        }
        .to_bytes()
        .unwrap()
    };
    let team = "2025_asset_groups/f1_2026_vehicle_package/teams/mclaren/wep/mclaren.erp";
    std::fs::create_dir_all(dir.join(team).parent().unwrap()).unwrap();
    std::fs::write(dir.join(team), handling()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, ops: &str, files: &[(&str, Vec<u8>)]| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::create_dir_all(src.join("registry")).unwrap();
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
    let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
    install(
        "mod.testring",
        r#"{"op":"track.add","source":"track"}"#,
        &[
            (
                "track/manifest.json",
                br#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" },
                     "ers": { "OverrideDetectionLapProgress": 0.91, "OverrideActivationLapProgress": 0.95 } }"#
                    .to_vec(),
            ),
            ("track/testring.erp", archive_of("WEP", &[world], 0)),
        ],
    );
    install(
        "t.car",
        &format!(r#"{{"op":"file.put","source":"u.erp","target":"{team}"}}"#),
        &[
            ("u.erp", handling()),
            (
                "registry/handling.json",
                br#"{ "vehicle.handling": { "own": { "vtf": "u.erp", "chassis": { "mass": 600 } } } }"#
                    .to_vec(),
            ),
        ],
    );
    let edits = |built: &Built| {
        let mut lines: Vec<String> = built
            .lines
            .iter()
            .filter(|l| l.starts_with("handling ") || l.starts_with("ers: "))
            .cloned()
            .collect();
        lines.sort();
        lines
    };
    let stages = |built: &Built| {
        ["handling", "ers"].map(|stage| std::fs::read(built.overlay.join(stage).join(team)).unwrap())
    };

    let first = build(&store, "default", &game).unwrap();
    let lines = edits(&first);
    assert!(
        lines.contains(
            &"ers: 1 handling file(s) in car archives addons put get the tracks' settings".to_owned()
        ),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("Chassis/Mass: 771 -> 600")),
        "{lines:?}"
    );
    assert_eq!(first.plan.data[team], first.overlay.join("ers").join(team));
    let before = stages(&first);
    let cache = store.root.join("cache").join(crate::cache::ARCHIVES_DIR);
    let kept = crate::cache::archive_entry::kept_archives(&cache);
    assert!(kept.len() >= 2, "{} archive(s) in the cache", kept.len());
    let used = || kept.iter().filter(|k| k.with_extension("used").is_file()).count();
    assert_eq!(used(), 0);
    std::fs::remove_dir_all(&first.overlay).unwrap();

    let second = build(&store, "default", &game).unwrap();
    assert_eq!(edits(&second), lines);
    assert_eq!(second.plan.data[team], second.overlay.join("ers").join(team));
    assert_eq!(stages(&second), before);
    assert_eq!(used(), kept.len(), "the rebuild links every edit the cache keeps");
    #[cfg(unix)]
    for stage in ["handling", "ers"] {
        use std::os::unix::fs::MetadataExt;
        let inode = |p: &std::path::Path| std::fs::metadata(p).unwrap().ino();
        let out = inode(&second.overlay.join(stage).join(team));
        assert!(
            kept.iter().any(|k| inode(k) == out),
            "the {stage} edit of the put archive is not the one the cache keeps"
        );
    }

    for k in &kept {
        let mut f = std::fs::File::options().append(true).open(k).unwrap();
        std::io::Write::write_all(&mut f, b"KEPT").unwrap();
    }
    std::fs::remove_dir_all(&second.overlay).unwrap();
    let third = build(&store, "default", &game).unwrap();
    assert_eq!(edits(&third), lines);
    assert_eq!(
        stages(&third),
        before,
        "an edit whose cached bytes changed is made again, not linked"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rebuild_takes_the_placement_handling_and_ers_fragments_from_the_frame_cache() {
    use crate::erp::pack::frame_cache;
    use swofty_formats::bxml::{Document, Element, Kind};
    use swofty_formats::erp::{Erp, HEADER_LEN, MAGIC};
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-car-frames-{}", std::process::id()));
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
    let el = |name: &str, attrs: &[(&str, &str)], children: Vec<Element>| Element {
        kind: Kind::Element,
        pad: 0,
        name: name.as_bytes().to_vec(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect(),
        children,
        end_pad: 0,
    };
    let leaf = |name: &str, v: &str| el(name, &[("value", v)], Vec::new());
    let uri = "eaid://f1_2026_vehicle_package/teams/mclaren/handling/mclaren.vtf";
    let handling = |mass: &str| {
        let doc = Document {
            root: el(
                "VTF",
                &[],
                vec![
                    el("Chassis", &[], vec![leaf("Mass", mass)]),
                    el(
                        "EnergyRecoverySystem",
                        &[("numTrackSettingsTypes", "1")],
                        vec![el(
                            "TrackSettings",
                            &[("name", "0")],
                            vec![
                                leaf("Track", "melbourne"),
                                leaf("MediumDeployFactor", "0.8"),
                                leaf("OverrideDetectionLapProgress", "0.8"),
                                leaf("OverrideActivationLapProgress", "0.85"),
                            ],
                        )],
                    ),
                    el(
                        "Padding",
                        &[],
                        (0..900)
                            .map(|i| leaf(&format!("Pad{i}"), "0.123456789"))
                            .collect(),
                    ),
                ],
            ),
            tail: Vec::new(),
        };
        let data = doc.to_bytes();
        assert!(data.len() > 16 << 10, "the handling file is {} bytes", data.len());
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: vec![frame_cache::zstd_resource(uri, "VTF", &data)],
        }
        .to_bytes()
        .unwrap()
    };
    let team = "2025_asset_groups/f1_2026_vehicle_package/teams/mclaren/wep/mclaren.erp";
    std::fs::create_dir_all(dir.join(team).parent().unwrap()).unwrap();
    std::fs::write(dir.join(team), handling("771")).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let install = |id: &str, ops: &str, files: &[(&str, Vec<u8>)]| {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(src.join("track")).unwrap();
        std::fs::create_dir_all(src.join("registry")).unwrap();
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
    let world = "eaid://environment_package/tracks/testring/wep/testring_common.wep";
    let spline = "eaid://environment_package/tracks/testring/ai/testring_ai_track.aispline";
    let track = || {
        let mut line = crate::activation::ailine::tests::aispline(|_| (-4.1, 3.0), 7);
        line.root.children.push(el(
            "padding",
            &[(
                "value",
                &(0..20u32 << 10)
                    .map(|i| char::from(b'0' + (i % 7) as u8))
                    .collect::<String>(),
            )],
            Vec::new(),
        ));
        let line = line.to_bytes();
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        let stock = Erp::parse(&archive_of("WEP", &[world], 0)).unwrap();
        Erp {
            header,
            version: 3,
            resources: stock
                .resources
                .into_iter()
                .chain([frame_cache::zstd_resource(spline, "AISplineData", &line)])
                .collect(),
        }
        .to_bytes()
        .unwrap()
    };
    install(
        "mod.testring",
        r#"{"op":"track.add","source":"track"}"#,
        &[
            (
                "track/manifest.json",
                br#"{ "track_code": "testring", "track_name": "Test Ring", "erp": "testring.erp",
                     "activation": { "copy_from_track": "melbourne", "copy_slot": "melbourne" },
                     "ers": { "OverrideDetectionLapProgress": 0.91, "OverrideActivationLapProgress": 0.95 } }"#
                    .to_vec(),
            ),
            ("track/testring.erp", track()),
        ],
    );
    let season = "2025_asset_groups/cs_2019_vehicle_package/teams/mclaren/wep/mclaren.erp";
    install(
        "t.cars",
        &format!(r#"{{"op":"file.put","source":"own.erp","target":"{season}"}}"#),
        &[
            ("own.erp", handling("700")),
            (
                "registry/handling.json",
                format!(
                    r#"{{ "vehicle.handling": {{
                        "stock_mclaren": {{ "vtf": {{ "stock": "{team}" }}, "chassis": {{ "mass": 650 }} }},
                        "own": {{ "vtf": "own.erp", "chassis": {{ "mass": 600 }} }}
                    }} }}"#
                )
                .into_bytes(),
            ),
        ],
    );

    let first = build(&store, "default", &game).unwrap();
    for line in [
        "ers: 1 handling file(s) in 1 car archive(s) get the tracks' settings",
        "ers: 1 handling file(s) in car archives addons put get the tracks' settings",
    ] {
        assert!(first.lines.contains(&line.to_owned()), "{:?}", first.lines);
    }
    for change in ["Chassis/Mass: 771 -> 650", "Chassis/Mass: 700 -> 600"] {
        assert!(
            first.lines.iter().any(|l| l.contains(change)),
            "{change}: {:?}",
            first.lines
        );
    }
    assert_eq!(
        super::archives::cached_frames(&first, &game),
        (3, 0),
        "{:?}",
        first.lines
    );
    let cache = store.root.join("cache");
    assert!(frame_cache::mark(&cache.join(crate::cache::PACKED_DIR)) >= 3);
    std::fs::remove_dir_all(&first.overlay).unwrap();
    for kept in [crate::cache::ARCHIVES_DIR, "ers", "handling", "maps"] {
        std::fs::remove_dir_all(cache.join(kept)).ok();
    }
    let second = build(&store, "default", &game).unwrap();
    assert_eq!(
        super::archives::cached_frames(&second, &game),
        (3, 3),
        "a rebuild compresses no placement, handling or ERS fragment the first build compressed"
    );
    std::fs::remove_dir_all(&root).ok();
}
