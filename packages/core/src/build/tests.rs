use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::archives::{kept_menus_line, stray_asset_groups};
use super::failure::{game_refused_text, loader_missing};
use super::registry::{added_tables, early_stage_line, registering_addons};
use super::saved::built_files;
use super::*;
use crate::activation::place::{SLOTS, slot_file};
use crate::asset::{FileKind, GroupOp, OpFile};
use crate::baseline::{Baseline, Objects, hex};
use crate::erpbuild::Resized;
use crate::launcher::BuildCx;
use crate::manifest::AddonManifest;
use crate::profile::Profile;

#[test]
fn a_file_another_program_holds_gets_its_own_hint() {
    let (data, game) = (Path::new("/data/Swofty"), Path::new("/games/F1 25"));
    let held = game_refused_text(data, game, true);
    assert!(
        held.starts_with("another program has a file in /games/F1 25 open"),
        "{held}"
    );
    assert!(
        held.contains("F1_25.exe") && held.contains("/data/Swofty"),
        "{held}"
    );
    assert!(!held.contains("administrator"), "{held}");
    assert!(game_refused_text(data, game, false).contains("run Swofty Launcher as administrator"));
}

#[cfg(windows)]
#[test]
fn a_sharing_violation_in_the_game_folder_is_not_an_administrator_problem() {
    let (data, game) = (
        Path::new("C:/data/Swofty"),
        Path::new("E:/SteamLibrary/steamapps/common/F1 25"),
    );
    let held = Error::Io {
        path: game.join("2025_asset_groups/animation_package/scenes/incardriver/cs16/incardriver_f1.erp"),
        source: std::io::Error::from_raw_os_error(32),
    };
    let text = refused_fix(data, game, &held).unwrap();
    assert!(text.starts_with("another program has a file in"), "{text}");
}
use crate::asset::{AssetBatch, Op, Origin};
use crate::asset::{FileOp, Root, Source};
use crate::baseline::Identity;
use crate::launcher::{Addon, Builtin};

fn uncached_source_hash(hasher: &mut Sha256, store: &Store, ops: &[AssetOp]) {
    for src in ops.iter().flat_map(AssetOp::sources) {
        let path = src.resolve(&source_dir(store, "test.pack", src.root)).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        hasher.update(format!("{}|{}|{:?}", src.path, meta.len(), meta.modified().ok()));
        if meta.is_dir() {
            for (rel, p) in loader_files(&path).unwrap() {
                let meta = std::fs::metadata(&p).unwrap();
                hasher.update(format!("{rel}|{}|{:?}", meta.len(), meta.modified().ok()));
            }
        }
    }
}

#[test]
fn source_stamp_cache_preserves_digest_and_rechecks_each_batch() {
    let root = std::env::temp_dir().join(format!("swofty-source-stamps-{}", std::process::id()));
    let store = Store::new(&root);
    let dir = store.addon_data_dir("test.pack").join("assets");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.bin"), b"a").unwrap();
    let ops: Vec<_> = ["assets", "assets/a.bin", "assets"]
        .into_iter()
        .map(|path| {
            AssetOp::File(FileOp::Put {
                source: Source {
                    root: Root::Data,
                    path: path.into(),
                },
                target: "out.bin".into(),
            })
        })
        .collect();
    let digest = || {
        let mut cached = BuildHash::new();
        cached.sources(&store, "test.pack", &ops).unwrap();
        let mut old = Sha256::new();
        uncached_source_hash(&mut old, &store, &ops);
        let cached = cached.finalize();
        assert_eq!(cached, old.finalize());
        cached
    };
    let before = digest();
    std::fs::write(dir.join("a.bin"), b"changed by helper").unwrap();
    assert_ne!(before, digest());
    let before = digest();
    std::fs::write(dir.join("new.bin"), b"new").unwrap();
    assert_ne!(before, digest());
    std::fs::remove_file(dir.join("a.bin")).unwrap();
    assert!(BuildHash::new().sources(&store, "test.pack", &ops).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "manual repeated-source build fingerprint benchmark"]
fn benchmark_repeated_build_sources() {
    let root = std::env::temp_dir().join(format!("swofty-source-bench-{}", std::process::id()));
    let store = Store::new(&root);
    let dir = store.addons_dir().join("test.pack/assets");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..512 {
        std::fs::write(dir.join(format!("{i}.bin")), b"asset").unwrap();
    }
    let ops: Vec<_> = (0..128)
        .map(|i| {
            AssetOp::File(FileOp::Put {
                source: Source::addon("assets"),
                target: format!("out/{i}"),
            })
        })
        .collect();
    let start = std::time::Instant::now();
    let mut old = Sha256::new();
    uncached_source_hash(&mut old, &store, &ops);
    println!("128 references / 512 files, uncached: {:?}", start.elapsed());
    let start = std::time::Instant::now();
    let mut cached = BuildHash::new();
    cached.sources(&store, "test.pack", &ops).unwrap();
    println!("128 references / 512 files, cached: {:?}", start.elapsed());
    assert_eq!(cached.finalize(), old.finalize());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_a_core_with_the_rows_feature_takes_the_replaced_operations_of_addons_with_data_files() {
    let root = std::env::temp_dir().join(format!("swofty-replaced-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let features = root.join(crate::planner::FEATURES_FILE);
    std::fs::create_dir_all(features.parent().unwrap()).unwrap();
    let std = |id: &str, data: bool, registry: Option<&str>| crate::stdlib::deps::Std {
        id: id.into(),
        name: id.into(),
        version: semver::Version::new(1, 0, 0),
        registry: registry.map(|r| crate::stdlib::manifest::RegistryManifest::parse(r).unwrap()),
        lock: None,
        data_files: match data {
            true => vec!["registry/content.json".into()],
            false => Vec::new(),
        },
        records: Vec::new(),
        files: Default::default(),
        unreadable: Vec::new(),
        older_tracks: Vec::new(),
    };
    let replaces = r#"{ "replaced_operations": ["db/records.json"] }"#;
    let std16 = BTreeMap::from([
        ("a.season".to_owned(), std("a.season", true, Some(replaces))),
        ("b.nodata".to_owned(), std("b.nodata", false, Some(replaces))),
        (
            "c.plain".to_owned(),
            std("c.plain", true, Some(r#"{ "requires": [] }"#)),
        ),
        ("d.old".to_owned(), std("d.old", true, None)),
    ]);
    assert!(replaced_operations(&std16, None).is_empty(), "no loader");
    std::fs::write(&features, r#"{ "schema": 1, "features": { "db": "stable" } }"#).unwrap();
    assert!(
        replaced_operations(&std16, Some(&root)).is_empty(),
        "a core without the rows feature"
    );
    std::fs::write(
        &features,
        r#"{ "schema": 1, "features": { "db": "stable", "registry_rows": "stable" } }"#,
    )
    .unwrap();
    let r = replaced_operations(&std16, Some(&root));
    assert_eq!(r.keys().copied().collect::<Vec<_>>(), ["a.season"]);
    assert!(r["a.season"].replaces("db/records.json") && !r["a.season"].replaces("db/season.json"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn registry_records_turn_the_early_stage_on_whatever_the_database_route() {
    let std = |id: &str, data: bool, lock: Option<&str>, registry: bool| crate::stdlib::deps::Std {
        id: id.into(),
        name: format!("{id} name"),
        version: semver::Version::new(1, 0, 0),
        registry: registry.then(|| crate::stdlib::manifest::RegistryManifest::parse("{}").unwrap()),
        lock: lock.map(|l| crate::stdlib::lock::Lock::parse(l).unwrap()),
        data_files: match data {
            true => vec!["registry/teams.json".into()],
            false => Vec::new(),
        },
        records: Vec::new(),
        files: Default::default(),
        unreadable: Vec::new(),
        older_tracks: Vec::new(),
    };
    let registers = r#"{ "provides": { "team.identity": [ { "id": "dll.teams/hrt" } ] } }"#;
    let std16 = BTreeMap::from([
        ("a.data".to_owned(), std("a.data", true, None, true)),
        ("b.code".to_owned(), std("b.code", false, Some(registers), false)),
        ("c.needs".to_owned(), std("c.needs", false, Some("{}"), true)),
        ("d.off".to_owned(), std("d.off", true, None, false)),
    ]);
    let order: Vec<String> = ["swofty.core", "c.needs", "b.code", "a.data", "e.old"]
        .map(str::to_owned)
        .to_vec();
    let on = registering_addons(&order, &std16);
    assert_eq!(on, ["b.code name", "a.data name"]);
    assert_eq!(
        early_stage_line(&on),
        "registry: the core loads at the early stage for the registry records of b.code name, a.data name"
    );
    assert!(
        registering_addons(&order[..2], &std16).is_empty(),
        "a registry.json or a lock without registrations leaves it off"
    );
}

#[test]
fn a_missing_or_cut_short_file_names_its_addon_or_the_game() {
    let root = std::env::temp_dir().join(format!("swofty-named-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(root.join("data"));
    let game = root.join("game");
    let addon = store.addons_dir().join("mod.miami24");
    std::fs::create_dir_all(addon.join("track")).unwrap();
    std::fs::write(
        addon.join("swofty.json"),
        r#"{"schema_version":1,"id":"mod.miami24","name":"Igora Drive","version":"0.9.13","author":"t","compat":{"game":"f1_25"}}"#,
    )
    .unwrap();
    let io = |path: PathBuf, kind| Error::Io {
        path,
        source: std::io::Error::from(kind),
    };
    let gone = named(
        &store,
        &game,
        io(
            addon.join("track").join("miami24.erp"),
            std::io::ErrorKind::NotFound,
        ),
    );
    assert_eq!(
        gone.to_string(),
        "Igora Drive (mod.miami24) is missing its file track/miami24.erp. Turn Igora Drive off in this \
         profile, or remove it on the Addons page and install or import it again."
    );
    let short = named(
        &store,
        &game,
        Error::Formats(swofty_formats::Error::IoAt {
            path: addon.join("track/mipmaps/a.mipmaps"),
            source: std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
        }),
    );
    assert!(
        short
            .to_string()
            .starts_with("Igora Drive (mod.miami24) has an incomplete or damaged track/mipmaps/a.mipmaps."),
        "{short}"
    );
    let stock = named(
        &store,
        &game,
        io(
            game.join("2025_asset_groups/a.erp"),
            std::io::ErrorKind::UnexpectedEof,
        ),
    );
    assert!(
        stock
            .to_string()
            .starts_with("2025_asset_groups/a.erp in the game folder is incomplete or damaged; verify"),
        "{stock}"
    );
    let missing = named(
        &store,
        &game,
        io(
            game.join("character_package/a.mipmaps"),
            std::io::ErrorKind::NotFound,
        ),
    );
    assert!(
        missing
            .to_string()
            .starts_with("character_package/a.mipmaps is missing from the game folder; update F1 25"),
        "{missing}"
    );
    let denied = named(
        &store,
        &game,
        io(
            addon.join("track/miami24.erp"),
            std::io::ErrorKind::PermissionDenied,
        ),
    );
    assert!(matches!(denied, Error::Io { .. }), "{denied}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_backup_stops_the_build_and_sends_the_player_through_the_setup_again() {
    let _pin = crate::builtin::LoaderPin::none();
    let root = std::env::temp_dir().join(format!("swofty-build-backup-gone-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join("a.erp"), "stock").unwrap();
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    let records = vec![swofty_formats::gvdb::Record::new(
        "entrants.teams._team_instancecount_",
        0,
        "0",
    )];
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    crate::setup::reset(&store.root, &game, false).unwrap();
    crate::setup::finish(&store.root).unwrap();
    let objects = Objects {
        root: store.root.join("baseline/objects"),
    };
    let mut baseline = Baseline::new(&game.identity);
    let sha = baseline
        .record(&objects, &dir, "a.erp", &())
        .unwrap()
        .sha256
        .clone();
    baseline.save(&store.root).unwrap();

    build(&store, "default", &game).unwrap();
    assert!(crate::setup::done(&store.root));

    std::fs::remove_dir_all(objects.path(&sha).parent().unwrap()).unwrap();
    let e = build(&store, "default", &game).unwrap_err();
    assert_eq!(
        e.to_string(),
        "Swofty's backup of the original a.erp is missing from its data folder. Run the one-time setup \
         again: open Swofty Launcher and press Play, or run `swofty setup reset`, repair the game in the EA \
         app or verify the game files in Steam, then run `swofty setup done`"
    );
    assert!(!crate::setup::done(&store.root));
    assert!(
        build(&store, crate::profile::VANILLA, &game).is_ok(),
        "a build without addons reads no backups"
    );
    std::fs::remove_dir_all(&root).ok();
}

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
                    fragments: vec![Fragment {
                        name: *b"temp",
                        offset: 0,
                        size: 1,
                        flags: 0x10,
                        compression: STORED,
                        packed: 1,
                        raw: vec![0],
                    }],
                    hash: [0; 16],
                })
                .collect(),
        }
        .to_bytes()
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
    let fragment = |raw: Vec<u8>| Fragment {
        name: *b"temp",
        offset: 0,
        size: raw.len() as u64,
        flags: 0x10,
        compression: STORED,
        packed: raw.len() as u64,
        raw,
    };
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
    std::fs::write(dir.join(GVDB), swofty_formats::gvdb::Gvdb { records }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let video = "videos/cm_f1_sting.bk2";
    let staged = store.addon_cache_dir(CORE_ID).join(video);

    let built = build(&store, "default", &game).unwrap();
    assert_eq!(std::fs::read(&staged).unwrap(), crate::builtin::CORE_INTRO_VIDEO);
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
                &crate::builtin::core_manifest(),
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
fn the_tables_a_records_file_adds_rows_to_are_the_add_row_tables() {
    let root = std::env::temp_dir().join(format!("swofty-added-tables-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("records.json");
    std::fs::write(
        &file,
        r#"{ "schema_version": 1, "ops": [
             { "op": "set", "select": "a.b[_metadata_=x]", "field": "f", "value": "v" },
             { "op": "add-row", "table": "entrants.entrant_lists", "from": "entrants.entrant_lists.entrant_list[_metadata_=f1]", "set": {} },
             { "op": "add-row", "table": "entrants.drivers", "from": "entrants.drivers.driver[_metadata_=d]", "set": {} },
             { "op": "add-row", "table": "entrants.drivers", "from": "entrants.drivers.driver[_metadata_=d]", "set": {} } ] }"#,
    )
    .unwrap();
    assert_eq!(
        added_tables(&file).into_iter().collect::<Vec<_>>(),
        ["entrants.drivers", "entrants.entrant_lists"]
    );
    assert!(added_tables(&root.join("gone.json")).is_empty());
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
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
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
        std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
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

struct Rows(AddonManifest);

impl Addon for Rows {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let reply: swofty_addon::helper::Built = serde_json::from_str(
            r#"{ "ops": [{ "op": "db.add-row", "table": "entrants.teams",
                           "from": "entrants.teams.team[_metadata_=mclaren_26]",
                           "set": { "_metadata_": "team12_26" } }],
                 "fields": { "entrants.teams.team": ["_metadata_", "stat_id"] } }"#,
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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

#[cfg(unix)]
#[test]
fn rebuilding_a_profile_never_rewrites_a_file_a_session_has_linked() {
    use std::os::unix::fs::MetadataExt;
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-linked-{}", std::process::id()));
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
    let stock = Gvdb { records }.to_bytes();
    std::fs::write(dir.join(GVDB), &stock).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
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
    let out = built.plan.data[GVDB].clone();
    let made = std::fs::read(&out).unwrap();
    let lines = std::cell::RefCell::new(Vec::new());
    let say = |l: &str| lines.borrow_mut().push(l.to_owned());
    let inst = crate::install::Installer {
        out: &say,
        ..crate::install::Installer::new(&store.root, &game)
    };
    inst.apply(&built.plan).unwrap();
    let installed = dir.join(GVDB);
    assert_eq!(
        std::fs::metadata(&installed).unwrap().ino(),
        std::fs::metadata(&out).unwrap().ino()
    );
    assert_eq!(std::fs::read(&installed).unwrap(), made);

    std::fs::remove_file(built.overlay.join(SAVED_FILE)).unwrap();
    let again = build(&store, "default", &game).unwrap();
    assert_eq!(again.plan.data[GVDB], out);
    assert_eq!(std::fs::read(&installed).unwrap(), made);
    assert_ne!(
        std::fs::metadata(&installed).unwrap().ino(),
        std::fs::metadata(&out).unwrap().ino(),
        "the rebuild wrote a new file"
    );
    assert_eq!(std::fs::metadata(&installed).unwrap().nlink(), 1);

    inst.apply(&crate::install::Plan::vanilla()).unwrap();
    assert_eq!(std::fs::read(&installed).unwrap(), stock);
    assert_eq!(std::fs::read(&out).unwrap(), made);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cached_output_changed_in_place_to_the_same_size_is_rebuilt() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-touched-{}", std::process::id()));
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
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
    let out = built.plan.data[GVDB].clone();
    let made = std::fs::read(&out).unwrap();
    let again = build(&store, "default", &game).unwrap();
    assert!(again.lines.iter().any(|l| l.contains("reusing it")));

    let mut bad = made.clone();
    bad[0] ^= 0xff;
    let file = std::fs::OpenOptions::new().write(true).open(&out).unwrap();
    std::io::Write::write_all(&mut &file, &bad).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    drop(file);
    assert_eq!(std::fs::read(&out).unwrap().len(), made.len());

    let rebuilt = build(&store, "default", &game).unwrap();
    assert!(!rebuilt.lines.iter().any(|l| l.contains("reusing it")));
    assert_eq!(std::fs::read(&rebuilt.plan.data[GVDB]).unwrap(), made);
    std::fs::remove_dir_all(&root).ok();
}

static LENDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_room_launch_build_uses_the_hosts_ids_under_its_own_cache_key_and_keeps_row_ids_json() {
    let _lending = LENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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

const PUT: &str = "environment_package/tracks/t/mipmaps/a.mipmaps";

struct Putter(AddonManifest);

impl Addon for Putter {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let mut batch = Rows(self.0.clone()).build(cx)?;
        batch.ops.push(Op {
            op: AssetOp::File(FileOp::Put {
                source: Source {
                    root: Root::Data,
                    path: "mip".into(),
                },
                target: PUT.into(),
            }),
            origin: Origin {
                addon: cx.cx.addon.into(),
                item: None,
            },
        });
        Ok(batch)
    }
}

#[test]
fn reuses_an_unchanged_build_until_an_input_or_the_game_folder_changes() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-reuse-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let db = |label: &str| {
        let mut records = vec![Record::new("entrants.teams._team_instancecount_", 0, "1")];
        records.push(Record::new("entrants.teams.team_0._metadata_", 4, "mclaren_26"));
        records.push(Record::new("entrants.teams.team_0.stat_id", 0, label));
        Gvdb { records }.to_bytes()
    };
    std::fs::write(dir.join(GVDB), db("100")).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.put", "name": "Put", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Putter(manifest)),
        locked: false,
        default_on: true,
    }]);
    std::fs::create_dir_all(store.addon_data_dir("t.put")).unwrap();
    std::fs::write(store.addon_data_dir("t.put").join("mip"), "mip").unwrap();
    let reused = |b: &Built| b.lines.iter().any(|l| l.starts_with("build: nothing changed"));

    let first = build(&store, "default", &game).unwrap();
    assert!(!reused(&first));
    assert_eq!(first.plan.added[PUT], store.addon_data_dir("t.put").join("mip"));
    let again = build(&store, "default", &game).unwrap();
    assert!(reused(&again), "{:?}", again.lines);
    assert_eq!(again.buildhash, first.buildhash);
    assert_eq!(
        (&again.plan.data, &again.plan.added, &again.plan.loader),
        (&first.plan.data, &first.plan.added, &first.plan.loader)
    );
    assert_eq!(again.settings, first.settings);
    assert_eq!(again.warnings, first.warnings);

    // A file copied in by hand where the plan adds one makes the build
    // decide again, and it is now a game file to replace.
    std::fs::create_dir_all(dir.join(PUT).parent().unwrap()).unwrap();
    std::fs::write(dir.join(PUT), "by hand").unwrap();
    let by_hand = build(&store, "default", &game).unwrap();
    assert!(!reused(&by_hand));
    assert!(by_hand.plan.data.contains_key(PUT) && !by_hand.plan.added.contains_key(PUT));
    std::fs::remove_dir_all(dir.join("environment_package")).unwrap();

    let built = build(&store, "default", &game).unwrap();
    std::fs::write(&built.plan.data[GVDB], b"torn").unwrap();
    assert!(!reused(&build(&store, "default", &game).unwrap()));

    std::fs::write(dir.join(GVDB), db("101")).unwrap();
    let changed = build(&store, "default", &game).unwrap();
    assert!(!reused(&changed));
    assert_ne!(changed.buildhash, first.buildhash);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cancelled_build_stops_and_the_next_one_starts_fresh() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-cancel-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 4, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.put", "name": "Put", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Putter(manifest)),
        locked: false,
        default_on: true,
    }]);
    std::fs::create_dir_all(store.addon_data_dir("t.put")).unwrap();
    std::fs::write(store.addon_data_dir("t.put").join("mip"), "mip").unwrap();
    let reused = |b: &Built| b.lines.iter().any(|l| l.starts_with("build: nothing changed"));

    crate::busy::request_cancel(&store.root).unwrap();
    assert!(matches!(build(&store, "default", &game), Err(Error::Cancelled)));
    crate::busy::clear_cancel(&store.root);
    let fresh = build(&store, "default", &game).unwrap();
    assert!(!reused(&fresh), "{:?}", fresh.lines);
    assert!(reused(&build(&store, "default", &game).unwrap()));
    crate::busy::request_cancel(&store.root).unwrap();
    build_as(&store, "default", &game, true).unwrap();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_reused_build_keeps_its_screens_assets_and_plugins() {
    let root = std::env::temp_dir().join(format!("swofty-build-saved-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let loader = root.join("loader");
    let placed: BTreeMap<String, PathBuf> = [
        (
            "swofty/ui/swofty_settings.ui",
            root.join("overlay/swofty/ui/swofty_settings.ui"),
        ),
        ("swofty/assets.json", root.join("overlay/swofty/assets.json")),
        (
            "swofty/addons/t.x/native/x.dll",
            root.join("addons/t.x/native/x.dll"),
        ),
        (
            "swofty/addons/core/swofty_core.dll",
            loader.join("swofty/addons/core/swofty_core.dll"),
        ),
        ("version.dll", loader.join("version.dll")),
    ]
    .into_iter()
    .map(|(rel, src)| (rel.to_owned(), src))
    .collect();
    for src in placed.values() {
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(src, "x").unwrap();
    }

    let kept = built_files(&placed, Some(&loader));
    assert_eq!(
        kept.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "swofty/addons/t.x/native/x.dll",
            "swofty/assets.json",
            "swofty/ui/swofty_settings.ui"
        ]
    );
    assert_eq!(built_files(&placed, None), placed);

    let mut built = Built::default();
    built.plan.loader = kept.clone();
    let saved = Saved::of(&built, 0, 0).unwrap();
    let store = Store::new(root.join("data"));
    let game = Game {
        identity: Identity {
            build_id: "1".into(),
            exe_sha256: "a".repeat(64),
        },
        dir: root.join("game"),
        user_reg: None,
    };
    assert!(saved.usable(&store, &game));
    let mut reused = Built::default();
    Saved::of(&built, 0, 0).unwrap().restore(&mut reused).unwrap();
    for rel in kept.keys() {
        assert_eq!(reused.plan.loader.get(rel), kept.get(rel), "{rel}");
    }

    std::fs::remove_file(&kept["swofty/ui/swofty_settings.ui"]).unwrap();
    assert!(!saved.usable(&store, &game));
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
    let stock = Gvdb { records }.to_bytes();
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
    let stock = Gvdb { records }.to_bytes();
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
    use crate::livery_kits::{KIT_YEARS, TABLE};
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
    let stock = Gvdb { records }.to_bytes();
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
    let stock = Gvdb { records };
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

    let listed = crate::heads::for_profile(&store, &game, "default").unwrap().heads;
    let labels: Vec<&str> = listed.iter().map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["amy", "lando_norris", "own_head", "rio_haryanto"]);
    let rio = listed.iter().find(|h| h.label == "rio_haryanto").unwrap();
    assert_eq!(rio.addon.as_deref(), Some("mod.rio"));
    assert_eq!(rio.addon_name.as_deref(), Some("Rio facemod"));
    assert!(!rio.female);
    assert!(listed.iter().find(|h| h.label == "amy").unwrap().female);
    assert_eq!(crate::heads::installed(&store).len(), 4);
    store
        .update_profile("default", |p| p.set_enabled("mod.rio", false))
        .unwrap();
    let listed = crate::heads::for_profile(&store, &game, "default").unwrap().heads;
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
    let stock = Gvdb { records };
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
    let stock = Gvdb { records };
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

#[test]
fn a_track_record_places_the_files_only_and_the_build_writes_the_track_files() {
    use swofty_formats::gvdb::Record;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-trackrecord-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    let lending = LENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::row_ids::lend(Some(host));
    let lent = build(&store, "default", &game);
    crate::row_ids::lend(None);
    drop(lending);
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
                fragments: vec![Fragment {
                    name: *b"temp",
                    offset: 0,
                    size: data.len() as u64,
                    flags: 0x10,
                    compression: STORED,
                    packed: data.len() as u64,
                    raw: data,
                }],
                hash: [0; 16],
            }],
        }
        .to_bytes()
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
                fragments: vec![Fragment {
                    name: *b"temp",
                    offset: 0,
                    size: 4,
                    flags: 0x10,
                    compression: STORED,
                    packed: 4,
                    raw: b"data".to_vec(),
                }],
                hash: [0; 16],
            }],
        }
        .to_bytes()
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
                fragments: vec![Fragment {
                    name: *b"temp",
                    offset: 0,
                    size: data.len() as u64,
                    flags: 0x10,
                    compression: STORED,
                    packed: data.len() as u64,
                    raw: data,
                }],
                hash: [0; 16],
            }],
        }
        .to_bytes()
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
                fragments: vec![Fragment {
                    name: *b"temp",
                    offset: 0,
                    size: data.len() as u64,
                    flags: 0x10,
                    compression: STORED,
                    packed: data.len() as u64,
                    raw: data,
                }],
                hash: [0; 16],
            }],
        }
        .to_bytes()
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();

    let resource = |name: &str, kind: &str, fragments: Vec<Vec<u8>>| {
        let mut r = Resource {
            name: name.into(),
            kind: swofty_formats::erp::kind(kind).unwrap(),
            unk: vec![0; 4],
            fragments: fragments
                .into_iter()
                .map(|data| Fragment {
                    name: *b"temp",
                    offset: 0,
                    size: data.len() as u64,
                    flags: 0x10,
                    compression: STORED,
                    packed: data.len() as u64,
                    raw: data,
                })
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
        crate::conflicts::last(&store, "default").unwrap().pairs,
        built.report.pairs
    );
    std::fs::remove_dir_all(&root).ok();
}

fn archive(names: &[&str], byte: u8) -> Vec<u8> {
    archive_of("UILayout", names, byte)
}

fn archive_of(kind: &str, names: &[&str], byte: u8) -> Vec<u8> {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
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
                kind: swofty_formats::erp::kind(kind).unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment {
                    name: *b"temp",
                    offset: 0,
                    size: 1,
                    flags: 0x10,
                    compression: STORED,
                    packed: 1,
                    raw: vec![byte],
                }],
                hash: [0; 16],
            })
            .collect(),
    }
    .to_bytes()
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
fn a_missing_loader_is_named_instead_of_a_track_limit() {
    assert_eq!(
        loader_missing(Some(Path::new("/apps/swofty"))),
        "Swofty Launcher's game files (version.dll, swofty_core.dll) are missing from /apps/swofty; your antivirus may have removed them. Restore them or run the installer again"
    );
    assert!(loader_missing(None).contains("missing from the launcher's folder;"));
}

#[test]
fn the_build_log_names_the_exe_folder_and_the_loader() {
    assert_eq!(
        loader_line(Some(Path::new("/apps/swofty")), None),
        "loader: none; swofty.exe in /apps/swofty"
    );
    assert_eq!(
        loader_line(Some(Path::new("/apps/swofty")), Some(Path::new("/apps/loader"))),
        "loader: /apps/loader; swofty.exe in /apps/swofty"
    );
}

#[test]
fn missing_archive_warnings_count_the_files() {
    let rels: BTreeSet<String> = ["b/x.erp", "a/x.erp"].map(str::to_owned).into();
    assert_eq!(
        missing_archives("Cams", &rels),
        "Cams: archive changes skipped; 2 of the files it changes are not in F1 25, such as a/x.erp. \
         Turn Cams off if it doesn't work, and tell its author"
    );
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
            tx.send(crate::conflicts::check(&store, "default", &game))
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
    let report = crate::conflicts::last(&store, "default").unwrap();
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
    std::fs::remove_file(crate::conflicts::path(&store, "default")).unwrap();
    let again = build(&store, "default", &game).unwrap();
    assert!(
        again
            .lines
            .iter()
            .any(|l| l.starts_with("build: nothing changed"))
    );
    assert_eq!(
        crate::conflicts::last(&store, "default").unwrap().pairs,
        report.pairs
    );
    assert!(crate::conflicts::last(&store, "vanilla").is_none());
    assert!(
        crate::conflicts::check(&store, "vanilla", &game)
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
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
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
    let _pin = crate::builtin::LoaderPin::none();
    let root = std::env::temp_dir().join(format!("swofty-build-tsmd-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    let erp = "2025_asset_groups/environment_package/tracks/bahrain/wep/bahrain_common.erp";
    std::fs::create_dir_all(dir.join(erp).parent().unwrap()).unwrap();
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
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
    let fragment = |raw: Vec<u8>| Fragment {
        name: *b"temp",
        offset: 0,
        size: raw.len() as u64,
        flags: 0x10,
        compression: STORED,
        packed: raw.len() as u64,
        raw,
    };
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
    .to_bytes();
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
        .map(|r| (r.name.clone(), f.load(r).unwrap().fragments[0].raw.clone()))
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

#[test]
fn an_addon_turns_on_the_experimental_features_it_lists() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-features-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
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
        r#"{ "schema_version": 1, "id": "t.hud", "name": "Race Control", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" }, "experimental_features": ["overlay"] }"#,
    )
    .unwrap();
    store.install_addon(&src).unwrap();
    let assets = |built: &Built| {
        built
            .plan
            .loader
            .get("swofty/assets.json")
            .map(|p| serde_json::from_slice::<serde_json::Value>(&std::fs::read(p).unwrap()).unwrap())
    };
    let off = build(&store, "default", &game).unwrap();
    assert_eq!(assets(&off), None);
    store
        .update_profile("default", |p| p.set_enabled("t.hud", true))
        .unwrap();
    let on = build(&store, "default", &game).unwrap();
    assert_eq!(assets(&on).unwrap()["features"], serde_json::json!(["overlay"]));
    assert!(
        on.lines
            .contains(&"features: the experimental overlay feature is on for Race Control".to_owned()),
        "{:?}",
        on.lines
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn telemetry_is_on_only_for_an_addon_that_lists_it() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-telemetry-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    for (id, extra) in [
        ("t.timing", r#", "experimental_features": ["telemetry"]"#),
        ("t.plain", ""),
    ] {
        let src = root.join(id);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{id}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}{extra} }}"#
            ),
        )
        .unwrap();
        store.install_addon(&src).unwrap();
    }
    let assets = |built: &Built| {
        built
            .plan
            .loader
            .get("swofty/assets.json")
            .map(|p| serde_json::from_slice::<serde_json::Value>(&std::fs::read(p).unwrap()).unwrap())
    };
    store
        .update_profile("default", |p| p.set_enabled("t.plain", true))
        .unwrap();
    assert_eq!(assets(&build(&store, "default", &game).unwrap()), None);
    store
        .update_profile("default", |p| p.set_enabled("t.timing", true))
        .unwrap();
    let on = build(&store, "default", &game).unwrap();
    assert_eq!(assets(&on).unwrap()["features"], serde_json::json!(["telemetry"]));
    std::fs::remove_dir_all(&root).ok();
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
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
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
fn a_shader_pack_grades_with_the_look_its_settings_pick() {
    use swofty_formats::cube::Cube;
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-shaders-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let pack = |id: &str, name: &str, warm: &str| {
        let src = root.join(id);
        std::fs::create_dir_all(src.join("looks")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                 "category": "shaders", "experimental_features": ["shaders"], "compat": {{ "game": "f1_25" }},
                 "options": [
                   {{ "id": "look", "type": "choice", "default": "mono", "values": ["mono", "warm"], "binds": "db" }},
                   {{ "id": "strength", "type": "slider", "min": 0, "max": 100, "step": 10, "default": 50,
                     "binds": "db" }}
                 ],
                 "shader": {{ "lut": "looks/{{{{look}}}}.cube", "strength": "{{{{strength}}}}" }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(src.join("looks/mono.cube"), Cube::identity(2).to_text()).unwrap();
        std::fs::write(src.join("looks/warm.cube"), warm).unwrap();
        store.install_addon(&src).unwrap()
    };
    let looks = pack("t.looks", "Looks", &Cube::identity(3).to_text());
    store
        .update_profile("default", |p| p.set_enabled("t.looks", true))
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let assets: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&built.plan.loader["swofty/assets.json"]).unwrap()).unwrap();
    assert_eq!(assets["features"], serde_json::json!(["shaders"]));
    assert_eq!(
        assets["shader"],
        serde_json::json!({ "lut": "shaders/look.lut", "strength": 0.5, "addon": "Looks" })
    );
    let packed = std::fs::read(&built.plan.loader["swofty/shaders/look.lut"]).unwrap();
    assert_eq!(Cube::unpack(&packed).unwrap().table, Cube::identity(2).table);
    let line = |file: &str, size: usize, strength: u32| {
        format!(
            "shaders: Looks grades the picture with looks/{file} ({size}x{size}x{size}) at {strength}% strength"
        )
    };
    assert!(
        built.lines.contains(&line("mono.cube", 2, 50)),
        "{:?}",
        built.lines
    );

    store
        .update_profile("default", |p| {
            p.set_option(&looks, "look", "warm".into())?;
            p.set_option(&looks, "strength", 100.into()).map(drop)
        })
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(
        built.lines.contains(&line("warm.cube", 3, 100)),
        "{:?}",
        built.lines
    );

    let later = pack("t.later", "Later Looks", "LUT_3D_SIZE 2\n0 0 0\n");
    store
        .update_profile("default", |p| {
            p.set_enabled("t.later", true)?;
            p.set_option(&later, "look", "warm".into()).map(drop)
        })
        .unwrap();
    let err = build(&store, "default", &game).unwrap_err().to_string();
    assert!(
        err.contains("Later Looks: looks/warm.cube: 1 colour line(s), but LUT_3D_SIZE 2 needs 8"),
        "{err}"
    );
    store
        .update_profile("default", |p| {
            p.set_option(&later, "look", "mono".into()).map(drop)
        })
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let skipped =
        "Looks: its look is not applied; one shader look applies at a time, and Later Looks loads after it";
    assert!(
        built.warnings.contains(&skipped.to_owned()),
        "{:?}",
        built.warnings
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_template_shader_pack_builds() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-template-look-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let template = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/shader-pack");
    let m = store.install_addon(&template).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(&m.id, true))
        .unwrap();
    for look in ["monochrome", "overcast", "noir"] {
        store
            .update_profile("default", |p| p.set_option(&m, "look", look.into()).map(drop))
            .unwrap();
        let built = build(&store, "default", &game).unwrap();
        let line = format!(
            "shaders: Monochrome Looks grades the picture with looks/{look}.cube (17x17x17) at 100% strength"
        );
        assert!(built.lines.contains(&line), "{:?}", built.lines);
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_template_postfx_pass_compiles_into_the_build() {
    if loader_dir().is_some() || crate::postfx::dxc().is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-postfx-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let template = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/postfx-pass");
    let m = store.install_addon(&template).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(&m.id, true))
        .unwrap();
    store
        .update_profile("default", |p| {
            p.set_option(&m, "saturation", serde_json::json!(40)).map(drop)
        })
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let assets: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&built.plan.loader["swofty/assets.json"]).unwrap()).unwrap();
    assert_eq!(assets["features"], serde_json::json!(["postfx"]));
    let ns = "example.vignette-pass";
    let pass = |i: usize,
                name: &str,
                options: serde_json::Value,
                inputs: serde_json::Value,
                output: String| {
        serde_json::json!({ "shader": format!("postfx/{i:02}.dxil"), "name": name, "addon": "Vignette Pass",
                            "addon_id": ns, "options": options, "budget_ms": 1.5, "live": [],
                            "inputs": inputs, "output": output, "threads": [8, 8], "raw": false })
    };
    assert_eq!(
        assets["postfx"],
        serde_json::json!([
            pass(
                0,
                "vignette",
                serde_json::json!([60.0, 40.0]),
                serde_json::json!([]),
                String::new()
            ),
            pass(
                1,
                "glow-down",
                serde_json::json!([]),
                serde_json::json!(["original"]),
                format!("{ns}/half")
            ),
            pass(
                2,
                "glow-up",
                serde_json::json!([30.0]),
                serde_json::json!([format!("{ns}/half")]),
                String::new()
            ),
        ])
    );
    assert_eq!(
        assets["postfx_textures"],
        serde_json::json!([{ "name": format!("{ns}/half"), "scale": 0.5, "format": "rgba16f", "persistent": false }])
    );
    assert!(assets.get("postfx_toggles").is_none());
    for i in 0..3 {
        assert!(
            std::fs::read(&built.plan.loader[&format!("swofty/postfx/{i:02}.dxil")])
                .unwrap()
                .starts_with(b"DXBC")
        );
    }
    let dxil = std::fs::read(&built.plan.loader["swofty/postfx/00.dxil"]).unwrap();
    assert!(dxil.starts_with(b"DXBC"));
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.starts_with("postfx: Vignette Pass's pass vignette runs as pass 1 before the HUD")),
        "{:?}",
        built.lines
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn shader_effects_and_the_active_config_build_into_the_loader_plan() {
    if loader_dir().is_some() || crate::postfx::dxc().is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-shaders-effects-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    crate::shaders::tests::addon(
        &store,
        "t.look",
        "",
        &[(
            "forced.swfx",
            "name = \"Forced look\"\n[spark]\nintensity = 3\n\n[vignette]\n",
        )],
        true,
    );
    let library = crate::shaders::catalog(&store).effects.len();
    assert!(library > 1);
    let built = build(&store, "default", &game).unwrap();
    let read = |rel: &str| std::fs::read(&built.plan.loader[&format!("swofty/{rel}")]).unwrap();
    let effects: serde_json::Value = serde_json::from_slice(&read("shaders/effects.json")).unwrap();
    let list = effects["effects"].as_array().unwrap();
    assert_eq!(list.len(), library, "{:?}", built.warnings);
    let assets: serde_json::Value = serde_json::from_slice(&read("assets.json")).unwrap();
    assert!(
        assets["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "postfx"),
        "a running config turns the core's postfx feature on"
    );
    let spark = list.iter().find(|e| e["id"] == "spark").unwrap();
    assert_eq!(
        (spark["addon"].as_str(), spark["slots"].as_u64()),
        (Some("t.look"), Some(1))
    );
    assert_eq!(spark["passes"][0]["dxil"], "shaders/fx/spark-00.dxil");
    assert!(read("shaders/fx/spark-00.dxil").starts_with(b"DXBC"));
    let active: serde_json::Value = serde_json::from_slice(&read("shaders/active.json")).unwrap();
    assert_eq!(active["name"], "Forced look");
    assert_eq!(active["forced_by"], "t.look name");
    assert_eq!(active["source"], "addon:t.look/forced");
    assert_eq!(active["toggle_key"], "B");
    assert_eq!(active["instances"][0]["effect"], "spark");
    assert_eq!(active["instances"][0]["params"], serde_json::json!([3.0]));
    assert_eq!(active["instances"][1]["effect"], "vignette");
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.starts_with("shaders: config \"Forced look\" runs 2 effect(s) (")),
        "{:?}",
        built.lines
    );
    assert!(
        !built.warnings.iter().any(|w| w.starts_with("shaders")),
        "{:?}",
        built.warnings
    );

    let again = build(&store, "default", &game).unwrap();
    assert_eq!(
        again.buildhash, built.buildhash,
        "the live serial is not part of the hash"
    );
    crate::shaders::set_toggle_key(&store, "F9").unwrap();
    assert_ne!(
        build(&store, "default", &game).unwrap().buildhash,
        built.buildhash
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_config_with_errors_builds_with_a_warning_and_shaders_off() {
    if loader_dir().is_some() || crate::postfx::dxc().is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-shaders-bad-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let r = crate::shaders::create(&store, "Broken", Some("[no_such_effect]\n")).unwrap();
    let mut p = store.profile("default").unwrap();
    crate::shaders::set_active(&store, &mut p, Some(r)).unwrap();
    let built = build(&store, "default", &game).unwrap();
    assert!(
        !built.plan.loader.keys().any(|k| k.starts_with("swofty/shaders/")),
        "no shader files when nothing runs"
    );
    assert!(
        built
            .warnings
            .iter()
            .any(|w| w.starts_with("shaders: Broken line 2") && w.ends_with("shaders are off")),
        "{:?}",
        built.warnings
    );
    assert!(built.lines.iter().any(|l| l == "shaders: off"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_vanilla_profile_builds_no_shader_files() {
    let store =
        Store::new(std::env::temp_dir().join(format!("swofty-build-shaders-vanilla-{}", std::process::id())));
    let files = crate::shaders::build_files(&store, &crate::profile::Profile::vanilla());
    assert!(files.files.is_empty() && files.line.is_none() && files.warnings.is_empty());
}

#[test]
fn a_v2_postfx_addon_builds_live_slots_textures_and_toggles() {
    if loader_dir().is_some() || crate::postfx::dxc().is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-build-postfx2-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    let src = root.join("src");
    std::fs::create_dir_all(src.join("fx")).unwrap();
    std::fs::create_dir_all(src.join("tex")).unwrap();
    let template = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/postfx-pass/fx");
    std::fs::copy(template.join("glow.hlsl"), src.join("fx/glow.hlsl")).unwrap();
    std::fs::write(
        src.join("fx/raw.hlsl"),
        "#include \"swofty/postfx.hlsli\"\n\
         [numthreads(16, 16, 1)] void fill(uint3 id : SV_DispatchThreadID)\n\
         { if (all(id.xy < swofty_size)) swofty_output[id.xy] = option(0).xxxx; }\n",
    )
    .unwrap();
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 1, 2);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap();
    }
    std::fs::write(src.join("tex/dirt.png"), &png).unwrap();
    std::fs::write(
        src.join(crate::manifest::MANIFEST_FILE),
        r#"{ "schema_version": 1, "id": "x.fx2", "name": "Fx Two", "version": "1.0.0", "author": "a",
            "experimental_features": ["postfx"], "compat": { "game": "f1_25" },
            "options": [
                { "id": "amount", "type": "slider", "min": 0, "max": 100, "step": 1, "default": 20, "binds": "native" },
                { "id": "fixed", "type": "slider", "min": 0, "max": 100, "step": 1, "default": 7, "binds": "db" },
                { "id": "live_on", "type": "toggle", "default": true, "binds": "native" },
                { "id": "build_on", "type": "toggle", "default": false, "binds": "db" },
                { "id": "key", "type": "choice", "default": "F9", "binds": "native", "values": ["F9", "None"] }
            ],
            "postfx_toggle": "{{key}}",
            "postfx_textures": [
                { "name": "half", "scale": 0.5 },
                { "name": "dirt", "file": "tex/dirt.png" }
            ],
            "postfx": [
                { "shader": "fx/glow.hlsl", "entry": "down", "name": "d", "inputs": ["original", "dirt"], "output": "half" },
                { "shader": "fx/glow.hlsl", "entry": "up", "name": "u", "options": ["{{amount}}", "{{fixed}}", 3],
                  "inputs": ["half", "previous"], "enabled": "{{live_on}}",
                  "when": { "views": ["cockpit"], "skip": ["paused"] } },
                { "shader": "fx/glow.hlsl", "entry": "up", "name": "off", "enabled": "{{build_on}}" },
                { "shader": "fx/raw.hlsl", "raw": true, "entry": "fill", "threads": [16, 16], "name": "r", "options": [1] }
            ] }"#,
    )
    .unwrap();
    let m = store.install_addon(&src).unwrap();
    store
        .update_profile("default", |p| p.set_enabled(&m.id, true))
        .unwrap();
    store
        .update_profile("default", |p| {
            p.set_option(&m, "amount", serde_json::json!(55)).map(drop)
        })
        .unwrap();
    let built = build(&store, "default", &game).unwrap();
    let assets: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&built.plan.loader["swofty/assets.json"]).unwrap()).unwrap();
    let ns = "x.fx2";
    let passes = assets["postfx"].as_array().unwrap();
    assert_eq!(
        passes.len(),
        3,
        "the pass enabled by a db toggle that is off is left out"
    );
    assert_eq!(
        passes[0]["inputs"],
        serde_json::json!(["original", format!("{ns}/dirt")])
    );
    assert_eq!(passes[0]["output"], format!("{ns}/half"));
    assert_eq!(passes[1]["options"], serde_json::json!([55.0, 7.0, 3.0]));
    assert_eq!(
        passes[1]["live"],
        serde_json::json!([{ "slot": 0, "option": "amount" }])
    );
    assert_eq!(passes[1]["enabled"], "live_on");
    assert_eq!(
        passes[1]["when"],
        serde_json::json!({ "views": ["cockpit"], "skip": ["paused"] })
    );
    assert_eq!(
        passes[1]["inputs"],
        serde_json::json!([format!("{ns}/half"), "previous"])
    );
    assert!(passes[0].get("enabled").is_none() && passes[0].get("when").is_none());
    assert_eq!(passes[2]["raw"], true);
    assert_eq!(passes[2]["threads"], serde_json::json!([16, 16]));
    assert_eq!(passes[2]["output"], "");
    assert_eq!(
        assets["postfx_textures"],
        serde_json::json!([
            { "name": format!("{ns}/half"), "scale": 0.5, "format": "rgba16f", "persistent": false },
            { "name": format!("{ns}/dirt"), "format": "rgba8", "file": "postfx/tex/00.tex" }
        ])
    );
    assert_eq!(
        assets["postfx_toggles"],
        serde_json::json!([{ "addon_id": ns, "option": "key" }])
    );
    let tex = std::fs::read(&built.plan.loader["swofty/postfx/tex/00.tex"]).unwrap();
    let mut want = b"SWTX".to_vec();
    want.extend_from_slice(&1u32.to_le_bytes());
    want.extend_from_slice(&2u32.to_le_bytes());
    want.extend_from_slice(&0u32.to_le_bytes());
    want.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(tex, want);
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
    std::fs::write(dir.join(GVDB), Gvdb { records }.to_bytes()).unwrap();
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
