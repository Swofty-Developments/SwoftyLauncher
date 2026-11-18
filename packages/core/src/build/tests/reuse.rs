use super::*;

fn uncached_stamp(path: &Path) -> String {
    let meta = std::fs::metadata(path).unwrap();
    if meta.is_dir() {
        "dir".to_owned()
    } else if meta.len() <= 1 << 20 {
        let data = std::fs::read(path).unwrap();
        format!(
            "{}|sha256 {}",
            data.len(),
            crate::game_folder::baseline::hex(&Sha256::digest(&data))
        )
    } else {
        format!("{}|{:?}", meta.len(), meta.modified().ok())
    }
}

fn uncached_source_hash(hasher: &mut Sha256, store: &Store, ops: &[AssetOp]) {
    for src in ops.iter().flat_map(AssetOp::sources) {
        let path = src.resolve(&source_dir(store, "test.pack", src.root)).unwrap();
        hasher.update(format!("{}|{}", src.path, uncached_stamp(&path)));
        if path.is_dir() {
            for (rel, p) in loader_files(&path).unwrap() {
                hasher.update(format!("{rel}|{}", uncached_stamp(&p)));
            }
        }
    }
}

#[test]
fn a_source_rewritten_with_the_same_bytes_keeps_the_build_hash_and_a_large_one_keys_on_its_time() {
    let root = std::env::temp_dir().join(format!("swofty-source-rewrite-{}", std::process::id()));
    let store = Store::new(&root);
    let dir = store.addon_cache_dir("test.pack").join("build");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("grid.json"), br#"{"packages":{},"seats":{}}"#).unwrap();
    std::fs::write(dir.join("big.bin"), vec![7u8; (1 << 20) + 1]).unwrap();
    let put = |path: &str| {
        AssetOp::File(FileOp::Put {
            source: Source {
                root: Root::Cache,
                path: path.into(),
            },
            target: "out.bin".into(),
        })
    };
    let ops = [put("build/grid.json"), put("build"), put("build/big.bin")];
    let digest = || {
        let mut h = BuildHash::new();
        h.sources(&store, "test.pack", &ops).unwrap();
        h.finalize()
    };
    let at = |secs: u64| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    let touch = |name: &str, secs: u64| {
        let f = std::fs::File::options().write(true).open(dir.join(name)).unwrap();
        f.set_modified(at(secs)).unwrap();
    };
    touch("grid.json", 1_000_000);
    let before = digest();
    std::fs::write(dir.join("grid.json"), br#"{"packages":{},"seats":{}}"#).unwrap();
    touch("grid.json", 2_000_000);
    assert_eq!(before, digest());
    std::fs::write(dir.join("grid.json"), br#"{"packages":{},"seats":{"1":2}}"#).unwrap();
    assert_ne!(before, digest());
    let before = digest();
    touch("big.bin", 3_000_000);
    assert_ne!(before, digest());
    std::fs::remove_dir_all(root).unwrap();
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
    let stock = Gvdb::new(records).to_bytes();
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
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
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
        Gvdb::new(records).to_bytes()
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

const HOOK_PUT: &str = "swofty/grid.json";

/// A build hook that writes its output file on every build, as Simplified's helper does, with the bytes
/// its data folder's `payload` holds.
struct Rewriter(AddonManifest);

impl Addon for Rewriter {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let mut batch = Rows(self.0.clone()).build(cx)?;
        let payload = std::fs::read(cx.cx.data_dir.join("payload")).expect("payload");
        let out = cx.cx.cache_dir.join("build/grid.json");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(&out, payload).unwrap();
        let file = std::fs::File::options().write(true).open(&out).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
            .unwrap();
        batch.ops.push(Op {
            op: AssetOp::File(FileOp::Put {
                source: Source {
                    root: Root::Cache,
                    path: "build/grid.json".into(),
                },
                target: HOOK_PUT.into(),
            }),
            origin: Origin {
                addon: cx.cx.addon.into(),
                item: None,
            },
        });
        Ok(batch)
    }
}

pub(super) fn reuse_fixture(name: &str) -> (std::path::PathBuf, Store, Game) {
    use swofty_formats::gvdb::Record;
    let root = std::env::temp_dir().join(format!("swofty-build-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    let records = vec![
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 4, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
    ];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.hook", "name": "Hook", "version": "1.0.0", "author": "t",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Rewriter(manifest)),
        locked: false,
        default_on: true,
    }]);
    std::fs::create_dir_all(store.addon_data_dir("t.hook")).unwrap();
    std::fs::write(store.addon_data_dir("t.hook").join("payload"), br#"{"seats":{}}"#).unwrap();
    (root, store, game)
}

fn reused(b: &Built) -> bool {
    b.lines.iter().any(|l| l.starts_with("build: nothing changed"))
}

#[test]
fn a_hook_that_rewrites_its_output_with_the_same_bytes_keeps_the_build_reused() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game) = reuse_fixture("hook-rewrite");
    let first = build(&store, "default", &game).unwrap();
    assert!(!reused(&first));
    let out = first.plan.added[HOOK_PUT].clone();
    let saved = first.overlay.join(SAVED_FILE);
    let written = std::fs::metadata(&saved).unwrap().modified().unwrap();
    for _ in 0..3 {
        let again = build(&store, "default", &game).unwrap();
        assert!(reused(&again), "{:?}", again.lines);
        assert_eq!(again.buildhash, first.buildhash);
        assert_eq!(again.plan.added[HOOK_PUT], out);
    }
    assert_eq!(std::fs::metadata(&saved).unwrap().modified().unwrap(), written);

    std::fs::write(
        store.addon_data_dir("t.hook").join("payload"),
        br#"{"seats":{1}}"#,
    )
    .unwrap();
    let changed = build(&store, "default", &game).unwrap();
    assert!(!reused(&changed));
    assert_ne!(changed.buildhash, first.buildhash);
    assert_eq!(
        std::fs::read(&changed.plan.added[HOOK_PUT]).unwrap(),
        br#"{"seats":{1}}"#
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reuse_follows_each_kind_of_input_change() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game) = reuse_fixture("input-kinds");
    let first = build(&store, "default", &game).unwrap();
    assert!(!reused(&first));

    let payload = store.addon_data_dir("t.hook").join("payload");
    std::fs::write(&payload, std::fs::read(&payload).unwrap()).unwrap();
    let touched = build(&store, "default", &game).unwrap();
    assert!(reused(&touched), "{:?}", touched.lines);
    assert_eq!(touched.buildhash, first.buildhash);

    store
        .update_profile("default", |p| p.set_enabled("t.hook", false))
        .unwrap();
    let off = build(&store, "default", &game).unwrap();
    assert_ne!(off.buildhash, first.buildhash);
    assert!(!off.plan.added.contains_key(HOOK_PUT));
    store
        .update_profile("default", |p| p.set_enabled("t.hook", true))
        .unwrap();
    // The build without it removed the profile's other build folders, so the same inputs are built again
    // under the same hash, and reused from then on.
    let back_on = build(&store, "default", &game).unwrap();
    assert!(!reused(&back_on), "{:?}", back_on.lines);
    assert_eq!(back_on.buildhash, first.buildhash);
    assert!(reused(&build(&store, "default", &game).unwrap()));

    std::fs::write(game.dir.join(crate::GAME_EXE), "exe, updated").unwrap();
    let updated = Game {
        identity: Identity::detect(&game.dir, Some(2)).unwrap(),
        dir: game.dir.clone(),
        user_reg: None,
    };
    let after_update = build(&store, "default", &updated).unwrap();
    assert!(!reused(&after_update));
    assert_ne!(after_update.buildhash, first.buildhash);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_output_touched_without_a_change_is_reused_and_one_changed_to_the_same_size_is_not() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game) = reuse_fixture("output-touch");
    let first = build(&store, "default", &game).unwrap();
    let out = first.plan.data[GVDB].clone();
    let made = std::fs::read(&out).unwrap();
    let rewrite = |bytes: &[u8]| {
        std::fs::write(&out, bytes).unwrap();
        let file = std::fs::File::options().write(true).open(&out).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(90))
            .unwrap();
    };
    rewrite(&made);
    assert!(reused(&build(&store, "default", &game).unwrap()));

    let mut bad = made.clone();
    bad[0] ^= 0xff;
    rewrite(&bad);
    let rebuilt = build(&store, "default", &game).unwrap();
    assert!(!reused(&rebuilt));
    assert_eq!(std::fs::read(&rebuilt.plan.data[GVDB]).unwrap(), made);
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
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
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

    crate::game_folder::busy::request_cancel(&store.root).unwrap();
    assert!(matches!(build(&store, "default", &game), Err(Error::Cancelled)));
    crate::game_folder::busy::clear_cancel(&store.root);
    let fresh = build(&store, "default", &game).unwrap();
    assert!(!reused(&fresh), "{:?}", fresh.lines);
    assert!(reused(&build(&store, "default", &game).unwrap()));
    crate::game_folder::busy::request_cancel(&store.root).unwrap();
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
fn a_build_looks_for_the_game_copies_only_for_the_first_start_import() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game) = reuse_fixture("no-looks");
    let looks = || crate::game_folder::copies::LOOKS.with(std::cell::Cell::get);
    let before = looks();
    assert!(!reused(&build(&store, "default", &game).unwrap()));
    assert_eq!(
        looks(),
        before + 1,
        "only the first-start import's check of an EA app copy"
    );
    let marker = crate::saves::launcher::marker_path(&store.root);
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, r#"{"at":0,"launcher":"1.10.0","profiles":[]}"#).unwrap();
    let before = looks();
    assert!(
        !reused(&build(&store, "default", &game).unwrap()),
        "the game now uses the launcher save"
    );
    assert!(reused(&build(&store, "default", &game).unwrap()));
    std::fs::write(store.addon_data_dir("t.hook").join("payload"), "changed").unwrap();
    assert!(!reused(&build(&store, "default", &game).unwrap()));
    assert_eq!(looks(), before, "past the first start");
    std::fs::remove_dir_all(&root).ok();
}
