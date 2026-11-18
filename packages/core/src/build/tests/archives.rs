use super::*;

#[test]
fn warns_about_stock_archives_that_still_carry_a_resized_resource() {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    let dir = std::env::temp_dir().join(format!("swofty-build-holders-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let ui = "2025_asset_groups/ui_package";
    let scene = "eaid://ui_package/art/screens/career/rnd_overview.idf";
    let archive = |names: &[&str]| {
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Erp {
            header,
            version: 3,
            resources: names
                .iter()
                .map(|n| Resource {
                    name: (*n).into(),
                    kind: swofty_formats::erp::kind("UILayout").unwrap(),
                    unk: vec![0; 4],
                    fragments: vec![Fragment::new(*b"temp", 1, 0x10, STORED, vec![0])],
                    hash: [0; 16],
                })
                .collect(),
        }
        .to_bytes()
        .unwrap()
    };
    for (rel, names) in [
        ("flow_career_hub.erp", vec![scene]),
        ("flow_team_hq.erp", vec!["eaid://ui_package/x.idf", scene]),
        ("screens/laptop.erp", vec![scene]),
        (
            "screens/other.erp",
            vec!["eaid://ui_package/art/screens/career/rnd_overview.idf_b"],
        ),
    ] {
        let path = dir.join(ui).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, archive(&names)).unwrap();
    }
    std::fs::write(dir.join(ui).join("notes.txt"), b"ERPK").unwrap();
    let resized = |erp: &str| Resized {
        erp: format!("{ui}/{erp}"),
        resources: vec![(scene.into(), "fragment 0 is 2 bytes, the stock one 1".into())],
    };
    assert_eq!(
        stock_holders(
            &dir,
            &[resized("flow_career_hub.erp"), resized("screens/laptop.erp")]
        ),
        [format!(
            "erp: {scene} is resized, but {ui}/flow_team_hq.erp also carries it and keeps the stock copy; replace it there too, or the game may show either one"
        )]
    );
    assert!(
        stock_holders(
            &dir,
            &[
                resized("flow_career_hub.erp"),
                resized("flow_team_hq.erp"),
                resized("screens/laptop.erp")
            ]
        )
        .is_empty()
    );
    std::fs::remove_dir_all(&dir).ok();
}

fn logo_archive(name: &str, slices: &[u8]) -> Vec<u8> {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    let mut head = vec![0u8; 40];
    head[12..16].copy_from_slice(&2u32.to_le_bytes());
    head[28..32].copy_from_slice(&(slices.len() as u32).to_le_bytes());
    let pixels: Vec<u8> = slices.iter().flat_map(|&b| [b; 4]).collect();
    let fragment = |raw: Vec<u8>| Fragment::new(*b"temp", raw.len() as u64, 0x10, STORED, raw);
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    Erp {
        header,
        version: 3,
        resources: vec![Resource {
            name: name.into(),
            kind: swofty_formats::erp::kind("GfxSurfaceRes").unwrap(),
            unk: vec![0; 4],
            fragments: vec![fragment(head), fragment(pixels)],
            hash: [0; 16],
        }],
    }
    .to_bytes()
    .unwrap()
}

#[test]
fn the_core_ships_the_startup_intro_and_a_later_addon_replaces_it() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-intro-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![swofty_formats::gvdb::Record::new(
        "entrants.teams._team_instancecount_",
        0,
        "0",
    )];
    std::fs::write(
        dir.join(GVDB),
        swofty_formats::gvdb::Gvdb::new(records).to_bytes(),
    )
    .unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let video = "videos/cm_f1_sting.bk2";
    let staged = store.addon_cache_dir(CORE_ID).join(video);

    let built = build(&store, "default", &game).unwrap();
    assert_eq!(
        std::fs::read(&staged).unwrap(),
        crate::addons::builtin::CORE_INTRO_VIDEO
    );
    let put = |b: &Built| b.plan.data.get(video).or(b.plan.added.get(video)).cloned();
    assert_eq!(put(&built), Some(staged));
    assert!(built.conflicts.is_empty());

    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.video", "name": "Video", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "files": [{ "source": "v.bk2", "target": "videos/cm_f1_sting.bk2" }], "assets": ["t.json"] } }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("t.json"),
        r#"{ "schema_version": 1, "ops": [{ "op": "lang.set", "language": "*", "key": "lng_subtitle_ea_sports_intro", "value": "MINE" }] }"#,
    )
    .unwrap();
    std::fs::write(src.join("v.bk2"), "mine").unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.video", true))
        .unwrap();
    let replaced = build(&store, "default", &game).unwrap();
    assert!(replaced.conflicts.is_empty(), "{:?}", replaced.conflicts);
    let mine = put(&replaced).unwrap();
    assert_eq!(std::fs::read(mine).unwrap(), b"mine");

    store
        .update_profile("default", |p| p.set_enabled("t.video", false))
        .unwrap();
    store
        .update_profile("default", |p| {
            p.set_option(
                &crate::addons::builtin::core_manifest(),
                "startup_intro",
                serde_json::json!(false),
            )
            .map(|_| ())
        })
        .unwrap();
    let off = build(&store, "default", &game).unwrap();
    assert_eq!(put(&off), None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn merged_slices_conflict_one_slice_at_a_time() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-slices-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    let ui = "2025_asset_groups/ui_package/common_front_end.erp";
    let logos = "eaid://ui_package/art/texture_arrays/t.texarray.image";
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups/ui_package")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![Record::new("entrants.teams._team_instancecount_", 0, "0")];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    std::fs::write(dir.join(ui), logo_archive(logos, &[0, 1, 2, 3])).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    for (id, slices) in [
        ("t.a", [0, 9, 2, 3]),
        ("t.b", [0, 1, 2, 8]),
        ("t.c", [0, 7, 2, 3]),
        ("t.same", [0, 1, 2, 3]),
    ] {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(&src).unwrap();
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
            format!(
                r#"{{ "schema_version": 1, "ops": [{{ "op": "erp.merge-slices", "erp": "{ui}", "resource": "{logos}", "source": "logos.erp" }}] }}"#
            ),
        )
        .unwrap();
        std::fs::write(src.join("logos.erp"), logo_archive(logos, &slices)).unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    }
    let built = build(&store, "default", &game).unwrap();
    let conflicts: Vec<String> = built.conflicts.iter().map(ToString::to_string).collect();
    assert_eq!(
        conflicts,
        [format!("slice 1 of {logos} in {ui}: t.c overrides t.a")]
    );
    for (id, n) in [("t.a", 1), ("t.b", 1), ("t.c", 1)] {
        let line = format!("erp: {id}: {n} of 4 slice(s) of {logos} merged into {ui}");
        assert!(built.lines.contains(&line), "{line}: {:?}", built.lines);
    }
    let file = swofty_formats::erp::ErpFile::open(&built.plan.data[ui]).unwrap();
    let merged = file.load(&file.erp.resources[0]).unwrap();
    let slices: Vec<u8> = merged.fragments[1]
        .data()
        .unwrap()
        .chunks(4)
        .map(|c| c[0])
        .collect();
    assert_eq!(slices, [0, 7, 2, 8]);
    store
        .update_profile("default", |p| {
            ["t.a", "t.b", "t.c"]
                .iter()
                .try_for_each(|id| p.set_enabled(id, false))
        })
        .unwrap();
    let same = build(&store, "default", &game).unwrap();
    assert!(!same.plan.data.contains_key(ui), "{:?}", same.plan.data);
    assert!(!same.overlay.join(ui).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn overlays_go_in_resource_by_resource_before_a_later_merge_into_them() {
    let root = std::env::temp_dir().join(format!("swofty-build-merged-later-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("ready.erp");
    let logos = "eaid://ui_package/t.texarray.image";
    std::fs::write(&path, logo_archive(logos, &[1, 2])).unwrap();
    let op = |op: &str| Op {
        op: serde_json::from_str(op).unwrap(),
        origin: Origin {
            addon: "t".into(),
            item: None,
        },
    };
    let ops = [
        op(r#"{"op":"erp.overlay","erps":["a.erp","b.erp"],"source":"ready.erp"}"#),
        op(&format!(
            r#"{{"op":"erp.merge-slices","erp":"b.erp","resource":"{logos}","source":"x.erp"}}"#
        )),
        op(&format!(
            r#"{{"op":"erp.merge-slices","erp":"c.erp","resource":"{logos}","source":"x.erp"}}"#
        )),
    ];
    let slices = slice_merges(&ops);
    assert_eq!(slices, [(1, "b.erp", logos), (2, "c.erp", logos)]);
    let targets = ["a.erp".to_owned(), "b.erp".to_owned()];
    assert_eq!(
        merged_later(&slices, 0, &targets, &path).unwrap(),
        Some(format!("a later erp.merge-slices merges into {logos} in b.erp"))
    );
    assert_eq!(merged_later(&slices, 1, &targets, &path).unwrap(), None);
    assert_eq!(merged_later(&slices, 0, &targets[..1], &path).unwrap(), None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn skips_archive_changes_to_files_the_game_does_not_have() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-absent-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::default().to_bytes()).unwrap();
    let erp = "2025_asset_groups/t.erp";
    let uri = "eaid://t/a.idf";
    std::fs::write(dir.join(erp), archive(&[uri], 0)).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "t.cams", "name": "Cams", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "assets": ["ops.json"] } }"#,
    )
    .unwrap();
    let put = |erp: &str| {
        format!(r#"{{"op":"erp.put-resource","erp":"{erp}","resource":"{uri}","source":"a.erp"}}"#)
    };
    std::fs::write(
        src.join("ops.json"),
        format!(
            r#"{{ "schema_version": 1, "ops": [{}, {}] }}"#,
            put(erp),
            put("2025_asset_groups/game_package/t.erp")
        ),
    )
    .unwrap();
    std::fs::write(src.join("a.erp"), archive(&[uri], 7)).unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.cams", true))
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.warnings.contains(
            &"Cams: archive changes skipped; 2025_asset_groups/game_package/t.erp is not a file in F1 25. \
              Turn Cams off if it doesn't work, and tell its author"
                .to_owned()
        ),
        "{:?}",
        built.warnings
    );
    assert!(built.plan.data.contains_key(erp), "{:?}", built.plan.data);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn files_in_an_asset_group_folder_at_the_game_root_are_flagged() {
    let root = std::env::temp_dir().join(format!("swofty-stray-groups-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let groups = root.join("2025_asset_groups");
    std::fs::create_dir_all(groups.join("f1_2026_vehicle_package")).unwrap();
    std::fs::create_dir_all(groups.join("character_package")).unwrap();
    std::fs::create_dir_all(root.join("swofty")).unwrap();
    let put = |addon: &str, target: &str| Op {
        op: serde_json::from_str(&format!(
            r#"{{"op":"file.put","source":"f","target":"{target}"}}"#
        ))
        .unwrap(),
        origin: Origin {
            addon: addon.to_owned(),
            ..serde_json::from_str(r#"{"addon":"x"}"#).unwrap()
        },
    };
    let files = [
        put("m", "f1_2026_vehicle_package/teams/ferrari/wep/ferrari.erp"),
        put("m", "f1_2026_vehicle_package/teams/ferrari/wep/b.erp"),
        put("m", "character_package/a.erp"),
        put("m", "2025_asset_groups/ui_package/ok.erp"),
        put("m", "swofty/own.bin"),
        put("m", "unknown_folder/x.bin"),
        put("m", "top_level.bin"),
    ];
    assert_eq!(
        stray_asset_groups(&root, &files),
        [
            ("m".to_owned(), "character_package".to_owned(), 1),
            ("m".to_owned(), "f1_2026_vehicle_package".to_owned(), 2),
        ]
    );
    assert_eq!(
        stray_asset_group_line("Mod", "character_package", 1),
        "Mod: 1 file(s) under character_package/ land at the game root, where the game never reads them; the folder belongs under 2025_asset_groups/ - re-import it with that path"
    );
    std::fs::remove_dir_all(&root).ok();
}

fn missing_resource_game(tag: &str) -> (PathBuf, Game, Store, String, String) {
    let root = std::env::temp_dir().join(format!("swofty-build-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::default().to_bytes()).unwrap();
    let provided = "2025_asset_groups/p.erp".to_owned();
    let stock = "2025_asset_groups/s.erp".to_owned();
    std::fs::write(dir.join(&stock), archive(&["eaid://s/a.idf"], 0)).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    (root, game, store, provided, stock)
}

fn install_ops(store: &Store, root: &Path, id: &str, name: &str, ops: &str, files: &[(&str, Vec<u8>)]) {
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
    for (file, bytes) in files {
        std::fs::write(src.join(file), bytes).unwrap();
    }
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(id, true))
        .unwrap();
}

/// The changed zstd fragments of at least the frame cache's 16 KiB in the archives a build wrote: how many
/// there are, and how many came from a cache [`frame_cache::mark`] marked.
pub(super) fn cached_frames(built: &Built, game: &Game) -> (usize, usize) {
    use crate::erp::pack::frame_cache;
    use swofty_formats::erp::Erp;
    let (mut changed, mut marked) = (0, 0);
    for (rel, path) in built.plan.data.iter().chain(&built.plan.added) {
        if !rel.ends_with(".erp") {
            continue;
        }
        let stock = std::fs::read(game.dir.join(rel))
            .ok()
            .and_then(|b| Erp::parse(&b).ok());
        let written = Erp::parse(&std::fs::read(path).unwrap()).unwrap();
        for r in &written.resources {
            let before = stock
                .as_ref()
                .and_then(|e| e.resources.iter().find(|s| s.name == r.name));
            for (i, f) in r.fragments.iter().enumerate() {
                let kept = before
                    .and_then(|s| s.fragments.get(i))
                    .is_some_and(|s| s.raw() == f.raw());
                if kept || !f.is_zstd() || f.size < 16 << 10 {
                    continue;
                }
                changed += 1;
                marked += usize::from(f.raw().is_some_and(frame_cache::marked));
            }
        }
    }
    (changed, marked)
}

#[test]
fn a_rebuild_takes_every_changed_fragment_from_the_frame_cache() {
    use crate::erp::pack::frame_cache;
    use swofty_formats::erp::{Erp, HEADER_LEN, MAGIC};
    let _pin = crate::addons::builtin::LoaderPin::none();
    let root = std::env::temp_dir().join(format!("swofty-build-frames-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::default().to_bytes()).unwrap();
    let noise = |seed: u8| -> Vec<u8> {
        (0..20u32 << 10)
            .map(|i| ((i % 251) as u8 ^ (i / 1021) as u8).wrapping_add(seed))
            .collect()
    };
    let stock = "2025_asset_groups/s.erp";
    let (one, two) = ("eaid://s/one.mat", "eaid://s/two.mat");
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    let archive = Erp {
        header,
        version: 3,
        resources: vec![
            frame_cache::zstd_resource(one, "Material", &noise(1)),
            frame_cache::zstd_resource(two, "Material", &noise(2)),
        ],
    }
    .to_bytes()
    .unwrap();
    std::fs::write(dir.join(stock), archive).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let replace = |res: &str, src: &str| {
        format!(
            r#"{{"op":"erp.replace-resource","erp":"{stock}","resource":"{res}","fragment":0,"source":"{src}"}}"#
        )
    };
    install_ops(
        &store,
        &root,
        "t.edit",
        "Edit",
        &format!("{},{}", replace(one, "one.bin"), replace(two, "two.bin")),
        &[("one.bin", noise(3)), ("two.bin", noise(4))],
    );
    let first = build(&store, "default", &game).unwrap();
    assert_eq!(cached_frames(&first, &game), (2, 0), "{:?}", first.lines);
    let cache = store.root.join("cache");
    assert_eq!(frame_cache::mark(&cache.join(crate::cache::PACKED_DIR)), 2);
    std::fs::remove_dir_all(&first.overlay).unwrap();
    std::fs::remove_dir_all(cache.join(crate::cache::ARCHIVES_DIR)).ok();
    let second = build(&store, "default", &game).unwrap();
    assert_eq!(
        cached_frames(&second, &game),
        (2, 2),
        "a rebuild compresses no fragment the first build compressed"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_edit_of_a_resource_another_addons_archive_lacks_is_skipped() {
    if loader_dir().is_some() {
        return;
    }
    let (root, game, store, provided, _) = missing_resource_game("missing-resource");
    let have = "eaid://p/have.idf";
    let lack = "eaid://p/lack.idf";
    install_ops(
        &store,
        &root,
        "t.base",
        "Base",
        &format!(r#"{{"op":"file.put","source":"p.erp","target":"{provided}"}}"#),
        &[("p.erp", archive_of("Material", &[have], 1))],
    );
    install_ops(
        &store,
        &root,
        "t.edit",
        "Edit",
        &format!(
            r#"{{"op":"erp.replace-resource","erp":"{provided}","resource":"{lack}","fragment":0,"source":"x.bin"}},
               {{"op":"erp.replace-resource","erp":"{provided}","resource":"{have}","fragment":0,"source":"x.bin"}}"#
        ),
        &[("x.bin", vec![9; 4])],
    );
    let built = build(&store, "default", &game).unwrap();
    let warning = format!(
        "Edit: change skipped; {lack} is not in {provided}, which Base provides. Turn Edit off if it doesn't work, and tell its author"
    );
    assert!(built.warnings.contains(&warning), "{:?}", built.warnings);
    let file = swofty_formats::erp::ErpFile::open(&built.plan.added[&provided]).unwrap();
    let res = file.erp.find(have).unwrap();
    assert_eq!(file.load(res).unwrap().fragments[0].data().unwrap(), vec![9; 4]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_edit_of_a_resource_a_stock_archive_lacks_still_fails() {
    if loader_dir().is_some() {
        return;
    }
    let (root, game, store, provided, stock) = missing_resource_game("missing-resource-stock");
    install_ops(
        &store,
        &root,
        "t.base",
        "Base",
        &format!(r#"{{"op":"file.put","source":"p.erp","target":"{provided}"}}"#),
        &[("p.erp", archive(&["eaid://p/have.idf"], 1))],
    );
    install_ops(
        &store,
        &root,
        "t.edit",
        "Edit",
        &format!(
            r#"{{"op":"erp.replace-resource","erp":"{stock}","resource":"eaid://s/lack.idf","fragment":0,"source":"x.bin"}}"#
        ),
        &[("x.bin", vec![9; 4])],
    );
    let err = build(&store, "default", &game).unwrap_err().to_string();
    assert!(
        err.contains(&format!("applying t.edit: {stock}: eaid://s/lack.idf: not found")),
        "{err}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn records_and_reports_the_conflicts_between_addons() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-conflicts-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 4, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let erp = "2025_asset_groups/t.erp";
    let uri = "eaid://t/a.idf";
    std::fs::write(dir.join(erp), archive(&[uri, "eaid://t/b.idf"], 0)).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let stat = format!(
        "0x{:016x}",
        swofty_formats::hash::fnv1a64("entrants.teams.team_0.stat_id")
    );
    for (id, name, erp_op) in [
        (
            "t.first",
            "First",
            format!(r#"{{"op":"erp.put-resource","erp":"{erp}","resource":"{uri}","source":"a.erp"}}"#),
        ),
        (
            "t.second",
            "Second",
            format!(r#"{{"op":"erp.overlay","erps":["{erp}"],"source":"a.erp"}}"#),
        ),
    ] {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }},
                     "operations": {{ "assets": ["ops.json"], "files": [{{ "source": "a.dds", "target": "x/a.dds" }}] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(
                r#"{{ "schema_version": 1, "ops": [{{"op":"db.put-record","key":"{stat}","type":0,"value":"3135"}}, {erp_op}] }}"#
            ),
        )
        .unwrap();
        std::fs::write(src.join("a.erp"), archive(&[uri], 7)).unwrap();
        std::fs::write(src.join("a.dds"), name).unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    }
    let listing = |d: &Path| {
        let mut all: Vec<_> = loader_files(d).unwrap().into_iter().collect();
        all.sort();
        all
    };
    let before = listing(&dir);
    let held = lock(&store, "default").unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            tx.send(crate::profile::conflicts::check(&store, "default", &game))
                .unwrap()
        });
        let waited = rx.recv_timeout(std::time::Duration::from_millis(300));
        assert!(waited.is_err(), "the check ran while another build held the lock");
        drop(held);
    });
    let checked = rx.recv().unwrap().unwrap();
    assert_eq!(listing(&dir), before);
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built
            .lines
            .contains(&"First: 1 database value, 1 archive resource, 1 file".to_owned()),
        "{:?}",
        built.lines
    );
    assert!(
        built.warnings.contains(
            &"Second overrides First on 1 database value, 1 archive resource and 1 file".to_owned()
        ),
        "{:?}",
        built.warnings
    );
    assert!(
        !built.warnings.iter().any(|w| w.starts_with("conflict:")),
        "{:?}",
        built.warnings
    );
    let report = crate::profile::conflicts::last(&store, "default").unwrap();
    assert!(report.is_current(&store.profile("default").unwrap()));
    assert_eq!(report.pairs, checked.pairs);
    assert_eq!(report.pairs, built.report.pairs);
    let pair = &report.pairs[0];
    assert_eq!(
        (
            pair.winner.as_str(),
            pair.loser.as_str(),
            pair.database,
            pair.resources,
            pair.files
        ),
        ("t.second", "t.first", 1, 1, 1)
    );
    assert_eq!(
        pair.items,
        [
            format!("{erp}:{uri}"),
            "x/a.dds".to_owned(),
            "entrants.teams.team[_metadata_=mclaren_26].stat_id".to_owned()
        ]
    );
    std::fs::remove_file(crate::profile::conflicts::path(&store, "default")).unwrap();
    let again = build(&store, "default", &game).unwrap();
    assert!(
        again
            .lines
            .iter()
            .any(|l| l.starts_with("build: nothing changed"))
    );
    assert_eq!(
        crate::profile::conflicts::last(&store, "default").unwrap().pairs,
        report.pairs
    );
    assert!(crate::profile::conflicts::last(&store, "vanilla").is_none());
    assert!(
        crate::profile::conflicts::check(&store, "vanilla", &game)
            .unwrap()
            .pairs
            .is_empty()
    );
    std::fs::remove_dir_all(&root).ok();
}

// One addon puts `t.erp` whole and another replaces a resource in it with a
// bigger one; `edit_first` loads the edit before the put.
fn build_put_and_edit(name: &str, edit_first: bool) -> (Built, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("swofty-build-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::create_dir_all(dir.join("2025_asset_groups")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
    let erp = "2025_asset_groups/t.erp";
    std::fs::write(
        dir.join(erp),
        archive_of("Data", &["eaid://t/a.idf", "eaid://t/b.idf"], 0),
    )
    .unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let pack = (
        "t.pack",
        format!(r#""files": [{{ "source": "whole.erp", "target": "{erp}" }}]"#),
    );
    let edit = ("t.edit", r#""assets": ["ops.json"]"#.to_owned());
    let order = match edit_first {
        true => [edit, pack],
        false => [pack, edit],
    };
    for (id, operations) in order {
        let src = root.join("src").join(id);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{id}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ {operations} }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("ops.json"),
            format!(
                r#"{{ "schema_version": 1, "ops": [{{"op":"erp.replace-resource","erp":"{erp}","resource":"eaid://t/a.idf","source":"a.bin"}}] }}"#
            ),
        )
        .unwrap();
        std::fs::write(src.join("a.bin"), [9, 9]).unwrap();
        std::fs::write(
            src.join("whole.erp"),
            archive_of("Data", &["eaid://t/a.idf", "eaid://t/b.idf", "eaid://t/c.idf"], 5),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    }
    let built = build(&store, "default", &game).unwrap();
    let whole = store.addons_dir().join("t.pack").join("whole.erp");
    (built, root, whole)
}

#[test]
fn a_resource_edit_that_leaves_stale_tsmd_stats_gets_them_rebuilt() {
    use swofty_formats::bxml::{Document, Element, Kind};
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    let _pin = crate::addons::builtin::LoaderPin::none();
    let root = std::env::temp_dir().join(format!("swofty-build-tsmd-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    let erp = "2025_asset_groups/environment_package/tracks/bahrain/wep/bahrain_common.erp";
    std::fs::create_dir_all(dir.join(erp).parent().unwrap()).unwrap();
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
    let spline = |gate: &str| Document {
        root: Element {
            kind: Kind::Element,
            pad: 0,
            name: b"track".to_vec(),
            attrs: vec![(b"name".to_vec(), gate.as_bytes().to_vec())],
            children: Vec::new(),
            end_pad: 0,
        },
        tail: Vec::new(),
    };
    let fragment = |raw: Vec<u8>| Fragment::new(*b"temp", raw.len() as u64, 0x10, STORED, raw);
    let res = "eaid://environment_package/tracks/bahrain/ai/bahrain_ai_track.aispline";
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    let stock = spline("default");
    let bytes = Erp {
        header,
        version: 3,
        resources: vec![Resource {
            name: res.into(),
            kind: swofty_formats::erp::kind("AISplineData").unwrap(),
            unk: vec![0; 4],
            fragments: vec![fragment(stock.to_bytes()), fragment(stock.string_stats(1))],
            hash: [0; 16],
        }],
    }
    .to_bytes()
    .unwrap();
    std::fs::write(dir.join(erp), bytes).unwrap();
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
        r#"{ "schema_version": 1, "id": "t.ai", "name": "More Aggressive AI", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "operations": { "erp": ["ops.json"] } }"#,
    )
    .unwrap();
    std::fs::write(
        src.join("ops.json"),
        format!(
            r#"{{ "schema_version": 1, "ops": [{{"op":"replace-resource","erp":"{erp}","resource":"{res}","fragment":0,"source":"line.aispline"}}] }}"#
        ),
    )
    .unwrap();
    let edited = spline("a racing line edited by hand");
    std::fs::write(src.join("line.aispline"), edited.to_bytes()).unwrap();
    store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled("t.ai", true))
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.lines.iter().any(|l| l
            == "erp: fixed the size headers of 1 AI racing line(s) in More Aggressive AI so F1 25 reads them safely"),
        "{:?}",
        built.lines
    );
    assert!(
        !built
            .warnings
            .iter()
            .any(|w| w.contains("aispline") || w.contains("size header"))
    );
    if let Some(path) = built.plan.data.get(erp) {
        let f = swofty_formats::erp::ErpFile::open(path).unwrap();
        let r = f.load(f.erp.find(res).unwrap()).unwrap();
        assert_eq!(r.fragments[1].data().unwrap(), edited.string_stats(1));
    }

    let mut cut = edited.to_bytes();
    cut.truncate(cut.len() - 3);
    std::fs::write(src.join("line.aispline"), cut).unwrap();
    store.install_addon(&src).unwrap();
    let Err(e) = build(&store, "default", &game) else {
        panic!("the build should stop");
    };
    assert_eq!(
        e.to_string(),
        "More Aggressive AI would crash F1 25: its bahrain_ai_track.aispline doesn't match the size header the \
         game reads with it, and the launcher can't read the file to fix the header. Turn More Aggressive AI \
         off in this profile."
    );
    std::fs::remove_dir_all(&root).ok();
}

fn fragments(path: &Path) -> Vec<(String, Vec<u8>)> {
    let f = swofty_formats::erp::ErpFile::open(path).unwrap();
    f.erp
        .resources
        .iter()
        .map(|r| {
            (
                r.name.clone(),
                f.load(r).unwrap().fragments[0].raw().unwrap().to_vec(),
            )
        })
        .collect()
}

#[test]
fn a_later_resource_edit_applies_to_an_archive_put_whole() {
    if loader_dir().is_some() {
        return;
    }
    let (built, root, _) = build_put_and_edit("put-then-edit", false);
    let erp = "2025_asset_groups/t.erp";
    assert_eq!(
        fragments(&built.plan.data[erp]),
        [
            ("eaid://t/a.idf".to_owned(), vec![9, 9]),
            ("eaid://t/b.idf".to_owned(), vec![5]),
            ("eaid://t/c.idf".to_owned(), vec![5]),
        ]
    );
    assert!(!built.plan.added.contains_key(erp));
    let pair = &built.report.pairs[0];
    assert_eq!(
        (pair.winner.as_str(), pair.loser.as_str(), pair.resources),
        ("t.edit", "t.pack", 1)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_archive_put_whole_replaces_earlier_resource_edits() {
    if loader_dir().is_some() {
        return;
    }
    let (built, root, whole) = build_put_and_edit("edit-then-put", true);
    assert_eq!(built.plan.data["2025_asset_groups/t.erp"], whole);
    let pair = &built.report.pairs[0];
    assert_eq!(
        (pair.winner.as_str(), pair.loser.as_str(), pair.resources),
        ("t.pack", "t.edit", 1)
    );
    std::fs::remove_dir_all(&root).ok();
}
