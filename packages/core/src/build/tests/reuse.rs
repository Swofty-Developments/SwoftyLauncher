use super::*;

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
